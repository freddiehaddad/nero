//! Translated from `src/nvim/window.c` (tractable core only).
//!
//! `window.c` is neovim's window-management/layout file (thousands of
//! lines) - almost entirely dependent on window creation/splitting/
//! closing machinery and the display pipeline, not attempted here.
//! Translated: `win_fdccol_count` (needed by `move.c`'s window-column-
//! offset calculations, including the `'foldcolumn'=auto` forms); `valid_tabpage` (walks the real
//! `GLOBALS.first_tabpage`/`tp_next` linked list, matching `undo.rs`'s
//! `any_buf_is_changed`/`firstbuf`/`b_next` walk precedent);
//! `is_bottom_win` (walks the real `WinT.w_frame`/`FrameT.fr_parent`
//! window-layout tree, all already-translated struct shapes).
//!
//! Also translated: `tabpage_win_valid`/`win_valid`/
//! `win_find_by_handle`/`win_valid_any_tab`/`win_count` - each walks
//! the real `GLOBALS.firstwin`/`WinT.w_next` window list (within a
//! single tabpage) and/or `GLOBALS.first_tabpage`/`tp_next` tabpage
//! list (across all tabpages), matching `valid_tabpage`'s own
//! established walk precedent. `win_valid_any_tab`'s inner per-tabpage
//! check reuses `tabpage_win_valid` directly rather than
//! re-implementing the same window-list walk a second time - a
//! faithful simplification, not a drift: the original's own
//! `FOR_ALL_TAB_WINDOWS(tp, wp)` macro literally expands to
//! `FOR_ALL_TABS(tp) FOR_ALL_WINDOWS_IN_TAB(wp, tp)`, i.e. exactly
//! `tabpage_win_valid`'s own single-tabpage walk nested inside an
//! outer tabpage loop.
//!
//! Also translated: `win_has_winnr`/`win_get_tabwin`/`win_findbuf`
//! (all real `window.c` functions), plus `win_id2win`/`win_getid`/
//! `get_winnr` (originally `static` helpers in `eval/window.c`,
//! hosted here alongside their own `window.c` dependencies rather
//! than in `eval/funcs.rs` - same "helper logic lives near its own
//! dependencies, the builtin Vimscript-facing wrapper lives in
//! `funcs.rs`" precedent as `state.rs`'s `get_mode`/`funcs.rs`'s
//! `f_mode`). All 6 need the same window/tabpage-list walk already
//! established above, plus `WinT.w_config`'s already-translated
//! `hide`/`focusable` fields for `win_has_winnr`'s own floating-
//! window-aware numbering check. `get_winnr`'s own digit+direction
//! argument form (e.g. `winnr("3j")`) needs `win_vert_neighbor`/
//! `win_horz_neighbor` (real window-layout geometry) - now translated
//! (see below), so the full digit+direction form is real too.
//!
//! Also translated: `frame2win` (a trivial leaf-of-a-frame-tree walk)
//! and `win_vert_neighbor`/`win_horz_neighbor` (`window.c`'s own
//! frame-tree neighbor-navigation algorithms for `winnr("3j")`-style
//! window movement) - both walk the already-translated `FrameT`'s
//! `fr_parent`/`fr_prev`/`fr_next`/`fr_child`/`fr_layout`/`fr_win`
//! fields plus `WinT.w_wincol`/`w_wcol` (vertical)/`w_winrow`/`w_wrow`
//! (horizontal) for the "which child is under the cursor" sub-search -
//! all fields already existed. Neither needs `win_goto` (which would
//! also redraw/switch real editor focus, not translated) - they just
//! COMPUTE a candidate window, matching `get_winnr`'s own read-only
//! use exactly.
//!
//! Also translated: `frame_fixed_height`/`frame_fixed_width` (whether
//! a frame's height/width should not be changed because of
//! `'winfixheight'`/`'winfixwidth'` - a leaf reflects its own window's
//! option value directly; a `FR_ROW`/`FR_COL` frame is fixed if
//! ANY/ALL of its children are, per the original's own exact
//! structure, needing only already-real `WinT.w_onebuf_opt.wo_wfh`/
//! `wo_wfw` fields). Translated ahead of their real callers
//! (`win_equal_rec`/`frame_new_height`/`frame_new_width`, part of the
//! larger window-resizing/equalization subsystem, not translated yet)
//! since both are small, self-contained, and have no design freedom
//! to get wrong - matching this crate's established "translate ahead
//! of a real caller" precedent.
//!
//! Also translated: `frame_minheight`/`frame_minwidth` (the minimal
//! height/width a frame needs, using `'winminheight'`/`'winminwidth'`,
//! or `'winheight'`/`'winwidth'` for a specific "next current window",
//! via already-real `OPTION_VARS.p_wh`/`p_wmh`/`p_wiw`/`p_wmw` and
//! `WinT.w_winbar_height`/`w_hsep_height`/`w_status_height`/
//! `w_vsep_width`). Introduces [`NOWIN`], a real, non-null sentinel
//! pointer value (`(win_T *)-1` in the original, distinct from both
//! null and any genuine `*mut WinT`) meaning "don't reserve at least
//! one line/column for the current window", the original's own real
//! 3-way distinction (null/`NOWIN`/a real window) for the
//! `next_curwin` parameter, kept as a raw-pointer comparison rather
//! than an `Option`-based redesign, matching the original's own
//! genuine pointer-identity semantics exactly. `FR_ROW` sums
//! (side-by-side widths add up) for `frame_minheight` but takes the
//! max for `frame_minwidth` (a column-of-rows only needs its LARGEST
//! single child's width), the same ROW/COL role-swap already
//! established for `frame_fixed_height`/`frame_fixed_width`.
//!
//! Also translated: `check_can_set_curbuf_disabled`/
//! `check_can_set_curbuf_forceit` (`'winfixbuf'` checks) - each omits
//! the original's real `emsg` call, matching the established "skip the
//! deferred-subsystem side effect, keep the state/return value
//! correct" policy.
//!
//! Also translated: `frame_fix_width`/`frame_fix_height` (set a
//! frame's own `fr_width`/`fr_height` directly from the window it
//! contains, via already-real `WinT.w_width`/`w_vsep_width`/
//! `w_height`/`w_hsep_height`/`w_status_height`) - trivial, mechanical
//! one-liners with no design freedom of their own, translated ahead of
//! their real callers (`frame_new_height`/`frame_new_width`, part of
//! the larger window-resizing subsystem, not translated yet).
//!
//! Also translated: `window_layout_lock`/`window_layout_unlock`/
//! `frames_locked`/`check_split_disallowed_err`/
//! `check_split_disallowed`/`window_layout_locked_err`/
//! `window_layout_locked` - 3 new file-static depth counters
//! (`SPLIT_DISALLOWED`/`CLOSE_DISALLOWED`/`FRAME_LOCKED`, the last
//! only ever incremented/decremented by `winframe_remove` in the
//! original, not yet translated, so it stays `0` today) plus the
//! predicates reading them. Every real `emsg` display is omitted,
//! matching the established "skip the deferred message-display side
//! effect, keep the exact same return value" policy -
//! `check_split_disallowed_err`'s own return-value SENSE (`true` =
//! allowed) is the OPPOSITE of `window_layout_locked_err`'s (`true` =
//! locked), a real, deliberate distinction in the original's own two
//! "_err" variants, preserved exactly rather than unified.
//!
//! Also translated: `unuse_tabpage`/`use_tabpage` (save/restore the
//! current window-list state - `GLOBALS.topframe`/`firstwin`/
//! `lastwin`/`curwin`/`OPTION_VARS.p_ch` - into/from a `TabpageT`'s own
//! `tp_topframe`/`tp_firstwin`/`tp_lastwin`/`tp_curwin`/`tp_ch_used`
//! fields, in preparation for/after switching the current tabpage) -
//! every field already existed. Translated ahead of their real callers
//! (`enter_tabpage`/`leave_tabpage`, not translated yet), matching the
//! established "translate ahead of a real caller" precedent for small,
//! self-contained pieces with no design freedom of their own.
//!
//! Also translated: `one_window`/`last_window` (whether a window is
//! the only non-floating window in a tabpage, or in the whole
//! session), via already-real `WinT.w_next`/`w_floating`,
//! `TabpageT.tp_firstwin`/`tp_next`,
//! `crate::globals::GLOBALS.firstwin`/`first_tabpage`. The original's
//! own debug-only `assert()` in `one_window` is preserved as a
//! `debug_assert!`, matching this crate's established policy for real
//! internal-invariant checks.
//!
//! Also translated: `can_close_floating_windows` (whether every
//! floating window stacked at the top of a tabpage's window list can
//! be closed without losing unsaved changes), via already-real
//! `WinT.w_prev`/`w_floating`/`w_buffer`, `BufT.b_nwindows`,
//! `crate::undo::buf_is_changed`, `crate::buffer::buf_hide`,
//! `crate::context::is_ctx_win`. Its own debug-only `assert()` is
//! likewise preserved as a `debug_assert!`.
//!
//! Also translated: `leaving_window`/`entering_window` (prompt-buffer
//! Insert-mode bookkeeping run when switching away from/to a window -
//! only ever does anything for a prompt-type buffer, via already-real
//! `crate::buffer::bt_prompt`/`crate::context::is_ctx_win`,
//! `GLOBALS.restart_edit`/`mode_displayed`/`clear_cmdline`/`Ins`/
//! `State`, `BufT.b_prompt_insert`).
//!
//! Also translated: `trigger_winnewpre`/`do_autocmd_winclosed`/
//! `trigger_tabclosedpre` (autocmd-triggering wrappers around window/
//! tabpage lifecycle events), via already-real
//! `crate::autocmd::apply_autocmds`/`has_event`. `trigger_winnewpre`
//! is a genuine, faithful no-op today (nothing can register a
//! `WinNewPre` autocmd, matching `apply_autocmds`'s own already-real
//! empty-registry fast path - not a stub). `do_autocmd_winclosed`/
//! `trigger_tabclosedpre`'s own real autocmd-execution bodies
//! `unimplemented!()` - unreachable today, since `has_event` is always
//! false for every event - while their `recursive`-guard early returns
//! (real, function-local C `static`s, translated as file-static
//! `GlobalCell<bool>`s) are fully real and tested.
//!
//! Also translated: `global_winbar_height` (whether `'winbar'` is set
//! globally, via already-real `OPTION_VARS.p_wbr`) and
//! `get_maximum_wincount` (the maximum number of windows that fit
//! within a given height in a frame, via already-real `frame2win`,
//! `OPTION_VARS.p_wmh`, `WinT.w_winbar_height`,
//! `FrameT.fr_child`/`fr_next`/`fr_layout`).
//!
//! Also translated: `only_one_window` (whether the current tabpage
//! has only one "real" window, used for `:only`/`:qa`-style checks),
//! via already-real `crate::globals::GLOBALS.first_tabpage`/
//! `firstwin`/`curbuf`/`curwin`, `crate::buffer::bt_help`,
//! `crate::context::is_ctx_win`, `WinT.w_floating`/
//! `w_onebuf_opt.wo_pvw`/`w_next`.
//!
//! Also translated: `get_last_winid` (reads a new file-static
//! `LAST_WIN_ID`, matching `window.c`'s own `last_win_id`, only ever
//! incremented by `win_alloc`, not yet translated - so this stays at
//! its initial value forever today) and `win_locked` (reads
//! `WinT.w_locked` directly).
//!
//! Also translated: `merge_win_config`/`clear_float_config` -
//! `merge_win_config` collapses to a plain struct assignment: the
//! original's own `clear_virttext` calls (freeing `dst`'s OLD
//! `title_chunks`/`footer_chunks` virtual-text data before the
//! overwrite, to avoid a C-style memory leak) have NO Rust
//! equivalent, since `WinConfig`'s own fields are real, owned
//! `Vec`s already dropped automatically by the assignment - matching
//! the established "Rust's own ownership model already does what the
//! C free dance does manually" pattern. `clear_float_config`'s own
//! `free_fields` parameter therefore has no observable effect here
//! either (both of its original branches reduce to the identical
//! Rust assignment) - kept, unused, purely for signature fidelity.
//!
//! Also translated: `valid_tabpage_win` (whether a tabpage is valid
//! AND has at least one valid window), via already-real
//! `crate::globals::GLOBALS.first_tabpage`/`firstwin`/`curtab`,
//! `TabpageT.tp_next`/`tp_firstwin`, `WinT.w_next`,
//! `win_valid_any_tab` - matching `tabpage_win_valid`'s own
//! established "current tabpage uses `GLOBALS.firstwin`, any other
//! uses its own `tp_firstwin`" walk precedent.
//!
//! Also translated, from `window.h` (not `window.c` - a tiny, self-
//! contained enum needed by `option.c`'s `check_num_option_bounds`):
//! `MIN_COLUMNS`/`MIN_LINES`/`STATUS_HEIGHT`.
//!
//! Also translated: `check_lnums`/`check_lnums_nested`/`reset_lnums`
//! (via their own shared `check_lnums_both` helper) - correct/save/
//! restore the cursor line number and topline in every window showing
//! the current buffer, used around buffer-switch autocommands, via
//! already-real `crate::globals::GLOBALS.first_tabpage`/`firstwin`/
//! `curtab`/`curwin`/`curbuf`, `TabpageT.tp_next`/`tp_firstwin`,
//! `WinT.w_next`/`w_buffer`/`w_cursor`/`w_topline`/`w_save_cursor`/
//! `w_valid`, `BufT.b_ml.ml_line_count`, `crate::mark_defs::equalpos`,
//! matching `valid_tabpage_win`'s own established "walk every
//! tabpage, walk each one's own window list" nested-loop precedent
//! (the original's own `FOR_ALL_TAB_WINDOWS(tp, wp)` macro literally
//! expands to that exact nesting). Translated ahead of a real caller
//! (`ex_docmd.c`'s buffer-switching commands, not yet translated),
//! matching the established "translate ahead of a real caller"
//! precedent for small, self-contained pieces with no design freedom
//! of their own.
//!
//! Also translated: `frame_check_height`/`frame_check_width` (whether
//! every frame in a row/column has exactly a given height/width),
//! using the same `fr_child`/`fr_next` walk already established by
//! `frame_fixed_height`/`frame_fixed_width` - each only walks its own
//! `FR_ROW`/`FR_COL` case (matching the original's own single `if`
//! guard exactly; a mismatched child inside the OTHER layout kind is
//! genuinely invisible to this specific check, not a bug).
//!
//! Also translated: `check_colorcolumn` (parses `'colorcolumn'`'s
//! comma-separated column list, e.g. `"+1,-1,80"`, into
//! `WinT.w_p_cc_cols` - called when `'colorcolumn'`/`'textwidth'` is
//! changed), via already-real `WinT.w_onebuf_opt.wo_cc`/`w_buffer`,
//! `BufT.b_p_tw`, `charset.rs`'s `getdigits_int`,
//! `ascii_defs.rs`'s `ascii_isdigit`. Returns a plain `bool` (`true`
//! = valid, matching the original's own `NULL` return) rather than a
//! `Result<(), ()>`, matching `valid_name`/`check_ff_value`'s own
//! precedent for this exact "valid or not, no other payload" shape -
//! the message text itself (`e_invarg`) is omitted, matching the
//! established "skip the deferred message-display side effect, keep
//! the exact same return value" policy. `int_cmp` (a trivial `qsort`
//! comparator) needs no Rust equivalent at all: `Vec::sort_unstable`
//! already sorts the collected columns directly. The original's own
//! `xfree(wp->w_p_cc_cols)` before reassigning likewise needs no
//! equivalent - a plain Rust assignment to `Option<Vec<i32>>` already
//! drops the old `Vec` automatically. Translated ahead of its real
//! callers (`optionstr.c`'s `did_set_colorcolumn`/`did_set_textwidth`,
//! both still blocked on `optset_T`, the option-setting-context
//! struct only the not-yet-translated `:set` engine can construct),
//! matching this crate's established "translate ahead of a real
//! caller" precedent.
//!
//! Also translated: `lastwin_nofloating` (the last non-floating
//! window in a tabpage, walking `w_prev` past any floating windows at
//! the end of the list) and `last_stl_height` (the height reserved
//! for the last window's own status line, via `'laststatus'` and
//! [`one_window`]) - both via already-real
//! `crate::globals::GLOBALS.lastwin`/`firstwin`,
//! `TabpageT.tp_lastwin`, `WinT.w_prev`/`w_floating`,
//! `crate::option_vars::OPTION_VARS.p_ls`. `lastwin_nofloating`'s own
//! debug-only `assert()` (never pass `tp` explicitly equal to
//! `curtab` - pass `NULL` instead) is preserved as a `debug_assert!`,
//! matching this crate's established policy for real internal-
//! invariant checks.
//!
//! Also translated: `make_win_info_dict` (builds a
//! `{width, height, topline, topfill, leftcol, skipcol}` dict
//! describing a window's size/scroll-position change), via already-
//! real `crate::eval::typval::tv_dict_alloc`/`tv_dict_add_nr`/
//! `tv_dict_unref`. Translated ahead of its real caller
//! (`check_window_scroll_resize`, needing `event_ignored`'s real
//! `'eventignorewin'` parsing plus `list_T`/`dict_T` accumulation
//! across every window in a tab - not yet translated), matching this
//! crate's established "translate ahead of a real caller" precedent
//! for small, self-contained pieces with no design freedom of their
//! own.
//!
//! Also translated: `alt_tabpage` (the tabpage to switch to when
//! closing the current one - prefers the last-accessed tabpage if
//! `'tabclose'` says so and it's still valid, else the next tabpage,
//! else the previous one), via already-real
//! `crate::globals::GLOBALS.curtab`/`first_tabpage`/
//! `lastused_tabpage`, `TabpageT.tp_next`,
//! `crate::option_vars::OPTION_VARS.tcl_flags`/`opt_tcl_flag`, and
//! [`valid_tabpage`]. Translated ahead of its real caller
//! (`close_last_window_tabpage`, needing `goto_tabpage_tp`/
//! `win_close_othertab`/`entering_window`/`apply_autocmds` for
//! multiple events - a substantial window/tabpage-closing function,
//! not yet translated), matching the established "translate ahead of
//! a real caller" precedent.
//!
//! Also translated: `frame_append`/`frame_insert`/`frame_remove` (the
//! low-level frame-tree linked-list splice/unlink primitives, using
//! already-real `FrameT.fr_next`/`fr_prev`/`fr_parent`/`fr_child`).
//! Caught a real test-design mistake (not an implementation bug) via
//! a null-pointer-dereference crash on the first test run:
//! `frame_insert`'s own "no previous sibling" branch reads `frp`'s OWN
//! `fr_parent` field (`frp->fr_parent->fr_child = frp;`), NOT
//! `before`'s - a genuine, easy-to-miss precondition of the original
//! (the caller must already have set the new frame's own `fr_parent`
//! before insertion whenever it might become the new head) -
//! documented explicitly in `frame_insert`'s own safety doc rather
//! than silently worked around. Translated ahead of their real
//! callers (`win_split_ins`/`winframe_remove`, both part of the
//! not-yet-translated window-splitting/closing machinery), matching
//! the established "translate ahead of a real caller" precedent.
//!
//! Also translated: `frame_has_win` (whether a frame subtree contains
//! a given window, a pure recursive predicate over the already-real
//! `FrameT.fr_layout`/`fr_win`/`fr_child`/`fr_next`) and `set_fraction`
//! (recompute `WinT.w_fraction` - the cursor's relative vertical
//! position on a `0..=FRACTION_MULT` scale, used to keep the cursor's
//! relative position stable across a resize - via already-real
//! `WinT.w_wrow`/`w_view_height`, plus a new `FRACTION_MULT` constant
//! mirroring the original's own `#define`). Translated ahead of their
//! real callers (`win_equal_rec`'s own `next_curwin` membership checks
//! for the former, `win_split_ins`/`win_new_height` for the latter,
//! none yet translated), matching the established "translate ahead of
//! a real caller" precedent.
//!
//! Also translated: `win_altframe` (the frame that should be resized
//! to take over the space occupied by a window about to be closed),
//! via already-real `one_window`/`alt_tabpage`/`frame_fixed_width`/
//! `frame_fixed_height` (all translated earlier in this same file/
//! segment), `TabpageT.tp_curwin`, `WinT.w_frame`,
//! `crate::option_vars::OPTION_VARS.p_sb`/`p_spr`. Its own debug-only
//! `assert()` (never pass `tp` explicitly equal to `curtab`) is
//! preserved as a `debug_assert!`, matching `lastwin_nofloating`'s own
//! already-established precedent for this exact pattern. Translated
//! ahead of its real callers (`win_close`/`winframe_remove`, both
//! needing the whole window-closing machinery, not yet translated),
//! matching the established "translate ahead of a real caller"
//! precedent.
//!
//! Also translated: `cmd_with_count` (build a command string with an
//! optional count suffix appended, e.g. `"quit"` + count `3` becomes
//! `"quit3"`) - a pure string-formatting helper with no dependencies
//! at all, returning an owned `Vec<u8>` rather than writing into the
//! original's own fixed-size `bufp`/`bufsize` output buffer, matching
//! this crate's established "return an owned collection instead of a
//! bounded out-buffer" idiom. Translated ahead of its real caller
//! (`do_window`, the large Normal-mode window-command dispatcher, not
//! yet translated), matching the established "translate ahead of a
//! real caller" precedent.
//!
//! Also translated: `win_find_tabpage` (the tabpage containing a given
//! window, or null if not found), reusing the exact `first_tabpage`/
//! `tp_next` + per-tabpage window-list walk already established by
//! `check_lnums_both`/`valid_tabpage_win` (the original's own
//! `FOR_ALL_TAB_WINDOWS(tp, wp)` macro, walked identically here).
//! Translated ahead of its real caller (`win_set_buf`, needing
//! `RedrawingDisabled`/`ctx_switch`/real buffer-switching, not yet
//! translated), matching the established "translate ahead of a real
//! caller" precedent.
//!
//! Also translated: `did_set_winminheight`/`did_set_winminwidth` (the
//! `'winminheight'`/`'winminwidth'` `did_set_*` option-setting
//! callbacks - re-verified tractable now that `min_rows_for_all_tabpages`/
//! `frame_minwidth` both exist, unlike this session's own earlier,
//! more general "`did_set_*` callbacks are uniformly not tractable"
//! finding, which predates those two functions). Both take an
//! `optset_T *args` parameter the original itself marks
//! `FUNC_ATTR_UNUSED` (never read in either body), so `_args: &mut
//! crate::option_defs::OptsetT` is accepted purely for signature
//! fidelity, matching `OptDidSetCbT`'s own parameter/return shape
//! (`&mut OptsetT -> Option<&'static [u8]>`) - though each function
//! itself stays `unsafe fn` (dereferences `GLOBALS`' raw pointers via
//! `min_rows_for_all_tabpages`/`frame_minwidth`), unlike
//! `OptDidSetCbT`'s own plain safe `fn` type, so a small safe shim
//! will be needed once these are wired into a real `opt_did_set_cb`
//! entry. Both always return `None` (`NULL` in the original, meaning
//! "no error") - the real `emsg(_(e_noroom))` display is skipped
//! (message.c's pipeline not tractable), while the identical state
//! mutation (`OPTION_VARS.p_wmh`/`p_wmw` decremented until enough room
//! exists, or reaching `0`) is kept exactly, matching the established
//! `mf_write`/`ml_open`/`u_get_headentry` policy. Translated ahead of
//! their real caller (`did_set_option`'s dispatch table, needing the
//! generic `:set` engine, not yet translated), matching the
//! established "translate ahead of a real caller" precedent.
//!
//! Also translated: [`frame_add_hsep`] - adds a horizontal separator
//! to windows at the bottom of a frame, a trivial `w_hsep_height = 1`
//! sibling of the already-translated `frame_add_statusline` (same
//! FR_LEAF-sets-directly/FR_ROW-recurses-into-every-child/
//! FR_COL-only-recurses-into-the-last-child structure, needing no
//! not-yet-translated dependency at all). Its own real callers
//! (`win_remove_status_line`, `last_status_rec`) both remain blocked
//! on `win_new_height`/`resize_frame_for_status` - translated ahead
//! of them, matching the established "translate ahead of a real
//! caller" precedent. `frame_add_height`/`frame_set_vsep`
//! investigated and confirmed still blocked (both need
//! `frame_new_height`/`win_new_width`, real window-resizing
//! machinery, neither translated); `command_height`/`last_status`/
//! `last_status_rec` also confirmed still blocked for the same
//! reason (`win_comp_pos`/`win_fix_scroll`/`win_remove_status_line`/
//! `resize_frame_for_status`, none translated) - `int_cmp` needs no
//! Rust equivalent at all (a plain `qsort` comparator; Rust's own
//! `Vec::sort_by`/`i32::cmp` already replace it, and its only real
//! caller, the `'colorcolumn'` `did_set_*` callback, is not
//! translated).
//!
//! Deferred: everything else in the file.

use crate::buffer_defs::WinT;
use crate::eval::typval_defs::{TypvalT, TypvalValue};
use crate::globals::GlobalCell;

/// minimal columns for screen (`MIN_COLUMNS`).
pub const MIN_COLUMNS: i32 = 12;
/// minimal lines for screen (`MIN_LINES`).
pub const MIN_LINES: i32 = 2;
/// height of a status line under a window (`STATUS_HEIGHT`).
pub const STATUS_HEIGHT: i32 = 1;

/// Check if `win` is a pointer to an existing window in tabpage `tp`
/// (`tabpage_win_valid`).
///
/// # Safety
/// `tp`'s own window list (`tp_firstwin`/`w_next`, or
/// `GLOBALS.firstwin`/`w_next` when `tp == GLOBALS.curtab`) must
/// consist of valid, live `WinT` pointers.
#[must_use]
pub unsafe fn tabpage_win_valid(
    tp: *const crate::buffer_defs::TabpageT,
    win: *const WinT,
) -> bool {
    if win.is_null() {
        return false;
    }

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
        if std::ptr::eq(wp, win) {
            return true;
        }
        // SAFETY: forwarded from this function's own safety doc.
        wp = unsafe { &*wp }.w_next;
    }
    false
}

/// Check if `win` is a pointer to an existing window in the current
/// tab page (`win_valid`).
///
/// # Safety
/// Same as [`tabpage_win_valid`].
#[must_use]
pub unsafe fn win_valid(win: *const WinT) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let curtab = unsafe { crate::globals::GLOBALS.get_mut() }.curtab;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { tabpage_win_valid(curtab, win) }
}

/// Find window `handle` in the current tab page, or a null pointer if
/// not found (`win_find_by_handle`).
///
/// # Safety
/// `GLOBALS.firstwin`'s own `w_next` chain must consist of valid,
/// live `WinT` pointers.
#[must_use]
pub unsafe fn win_find_by_handle(handle: crate::types_defs::HandleT) -> *mut WinT {
    // SAFETY: forwarded from this function's own safety doc.
    let mut wp = unsafe { crate::globals::GLOBALS.get_mut() }.firstwin;
    while !wp.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { &*wp }.handle == handle {
            return wp;
        }
        // SAFETY: forwarded from this function's own safety doc.
        wp = unsafe { &*wp }.w_next;
    }
    std::ptr::null_mut()
}

/// Check if `win` is a pointer to an existing window in ANY tab page
/// (`win_valid_any_tab`).
///
/// # Safety
/// `GLOBALS.first_tabpage`'s own `tp_next` chain, and each tabpage's
/// own window list, must consist of valid, live pointers.
#[must_use]
pub unsafe fn win_valid_any_tab(win: *const WinT) -> bool {
    if win.is_null() {
        return false;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let mut tp = unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage;
    while !tp.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { tabpage_win_valid(tp, win) } {
            return true;
        }
        // SAFETY: forwarded from this function's own safety doc.
        tp = unsafe { &*tp }.tp_next;
    }
    false
}

/// Find window `handle` in ANY tab page, or a null pointer if not
/// found (`handle_get_window`, `api/private/helpers.h`). The original
/// is a `pmap_get(int)(&window_handles, (h))` lookup into a real,
/// global registry populated whenever a window is created/closed;
/// this crate has no such registry, so this instead walks every
/// tabpage's own window list directly - an observably identical
/// result for every window this crate can currently construct, since
/// every live window is reachable this way and nothing here can leave
/// a closed window's handle dangling in a registry (there is no
/// registry to leave it dangling in).
///
/// # Safety
/// `GLOBALS.first_tabpage`'s own `tp_next` chain, and each tabpage's
/// own window list, must consist of valid, live pointers.
#[must_use]
pub unsafe fn handle_get_window(handle: crate::types_defs::HandleT) -> *mut WinT {
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
            if unsafe { &*wp }.handle == handle {
                return wp;
            }
            // SAFETY: forwarded from this function's own safety doc.
            wp = unsafe { &*wp }.w_next;
        }
        // SAFETY: forwarded from this function's own safety doc.
        tp = unsafe { &*tp }.tp_next;
    }
    std::ptr::null_mut()
}

/// Return the number of windows in the current tab page (`win_count`).
///
/// # Safety
/// `GLOBALS.firstwin`'s own `w_next` chain must consist of valid,
/// live `WinT` pointers.
#[must_use]
pub unsafe fn win_count() -> i32 {
    let mut count = 0;
    // SAFETY: forwarded from this function's own safety doc.
    let mut wp = unsafe { crate::globals::GLOBALS.get_mut() }.firstwin;
    while !wp.is_null() {
        count += 1;
        // SAFETY: forwarded from this function's own safety doc.
        wp = unsafe { &*wp }.w_next;
    }
    count
}

/// Whether window `wp` "counts" toward window numbering in tab page
/// `tp` (`win_has_winnr`). `tp`'s own current window always counts;
/// otherwise, only a non-hidden, focusable window counts (a floating
/// window can be configured, via `w_config`, to not participate in
/// window numbering).
///
/// # Safety
/// `wp`/`tp` must be valid, non-null pointers to live `WinT`/
/// `TabpageT`.
#[must_use]
pub unsafe fn win_has_winnr(wp: *const WinT, tp: *const crate::buffer_defs::TabpageT) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let is_curtab = std::ptr::eq(tp, unsafe { crate::globals::GLOBALS.get_mut() }.curtab);
    let tab_curwin = if is_curtab {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::globals::GLOBALS.get_mut() }.curwin
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &*tp }.tp_curwin
    };
    // SAFETY: forwarded from this function's own safety doc.
    let w_config = &unsafe { &*wp }.w_config;
    std::ptr::eq(wp, tab_curwin) || (!w_config.hide && w_config.focusable)
}

/// Get the window number (within the CURRENT tab page only) for
/// window handle `id`, or `0` if not found (or found but not counted,
/// per [`win_has_winnr`]) (`win_id2win`, `eval/window.c`).
///
/// # Safety
/// `GLOBALS.firstwin`'s own `w_next` chain must consist of valid,
/// live `WinT` pointers.
#[must_use]
pub unsafe fn win_id2win(id: crate::types_defs::HandleT) -> i32 {
    let mut nr = 1;
    // SAFETY: forwarded from this function's own safety doc.
    let curtab = unsafe { crate::globals::GLOBALS.get_mut() }.curtab;
    // SAFETY: forwarded from this function's own safety doc.
    let mut wp = unsafe { crate::globals::GLOBALS.get_mut() }.firstwin;
    while !wp.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { &*wp }.handle == id {
            // SAFETY: forwarded from this function's own safety doc.
            return if unsafe { win_has_winnr(wp, curtab) } { nr } else { 0 };
        }
        // SAFETY: forwarded from this function's own safety doc.
        nr += i32::from(unsafe { win_has_winnr(wp, curtab) });
        // SAFETY: forwarded from this function's own safety doc.
        wp = unsafe { &*wp }.w_next;
    }
    0
}

/// Get the tab number and window number (`(tabnr, winnr)`) for window
/// handle `id`, both `0` if not found - or found, but not counted per
/// [`win_has_winnr`] (`win_get_tabwin`, `window.c`).
///
/// # Safety
/// `GLOBALS.first_tabpage`'s own `tp_next` chain, and each tabpage's
/// own window list, must consist of valid, live pointers.
#[must_use]
pub unsafe fn win_get_tabwin(id: crate::types_defs::HandleT) -> (i32, i32) {
    let mut tnum = 1;
    let mut wnum = 1;
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
            if unsafe { &*wp }.handle == id {
                // SAFETY: forwarded from this function's own safety doc.
                return if unsafe { win_has_winnr(wp, tp) } { (tnum, wnum) } else { (0, 0) };
            }
            // SAFETY: forwarded from this function's own safety doc.
            wnum += i32::from(unsafe { win_has_winnr(wp, tp) });
            // SAFETY: forwarded from this function's own safety doc.
            wp = unsafe { &*wp }.w_next;
        }
        tnum += 1;
        wnum = 1;
        // SAFETY: forwarded from this function's own safety doc.
        tp = unsafe { &*tp }.tp_next;
    }
    (0, 0)
}

/// Get the window handle for window number `winnr` in tab number
/// `tabnr` (`win_getid`, `eval/window.c`). `winnr == None` means "the
/// current window" (returns its handle directly, `tabnr` ignored,
/// matching the original's own `argvars[0].v_type == VAR_UNKNOWN`
/// early return). `0` if `winnr <= 0` or not found; `-1` if `tabnr`
/// doesn't resolve to a real tab page.
///
/// # Safety
/// Same requirement as [`win_get_tabwin`].
#[must_use]
pub unsafe fn win_getid(winnr: Option<i32>, tabnr: Option<i32>) -> crate::types_defs::HandleT {
    let Some(mut winnr) = winnr else {
        // SAFETY: forwarded from this function's own safety doc.
        return unsafe { &*crate::globals::GLOBALS.get_mut().curwin }.handle;
    };
    if winnr <= 0 {
        return 0;
    }

    let (tp, mut wp) = match tabnr {
        None => {
            // SAFETY: forwarded from this function's own safety doc.
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            (g.curtab, g.firstwin)
        }
        Some(mut tabnr) => {
            // SAFETY: forwarded from this function's own safety doc.
            let mut tp = unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage;
            while !tp.is_null() {
                tabnr -= 1;
                if tabnr == 0 {
                    break;
                }
                // SAFETY: forwarded from this function's own safety doc.
                tp = unsafe { &*tp }.tp_next;
            }
            if tp.is_null() {
                return -1;
            }
            // SAFETY: forwarded from this function's own safety doc.
            let is_curtab = std::ptr::eq(tp, unsafe { crate::globals::GLOBALS.get_mut() }.curtab);
            let wp = if is_curtab {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { crate::globals::GLOBALS.get_mut() }.firstwin
            } else {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { &*tp }.tp_firstwin
            };
            (tp, wp)
        }
    };

    while !wp.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        winnr -= i32::from(unsafe { win_has_winnr(wp, tp) });
        if winnr == 0 {
            // SAFETY: forwarded from this function's own safety doc.
            return unsafe { &*wp }.handle;
        }
        // SAFETY: forwarded from this function's own safety doc.
        wp = unsafe { &*wp }.w_next;
    }
    0
}

/// Find all windows (across all tab pages) currently showing buffer
/// `bufnr`, returning their handles in tab/window order (`win_findbuf`,
/// `eval/window.c`).
///
/// # Safety
/// Same requirement as [`win_get_tabwin`], plus each window's
/// `w_buffer` must be a valid, non-null pointer to a live `BufT`.
#[must_use]
pub unsafe fn win_findbuf(bufnr: i32) -> Vec<crate::types_defs::HandleT> {
    let mut found = Vec::new();
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
            let w = unsafe { &*wp };
            // SAFETY: forwarded from this function's own safety doc.
            if unsafe { &*w.w_buffer }.handle == bufnr {
                found.push(w.handle);
            }
            wp = w.w_next;
        }
        // SAFETY: forwarded from this function's own safety doc.
        tp = unsafe { &*tp }.tp_next;
    }
    found
}

/// Get the window number for `arg` in tab page `tp` (`get_winnr`,
/// `eval/window.c`). `arg == None` means no argument was given (the
/// common case - the CURRENT window's own number, `0` for a hidden/
/// non-focusable window unless it IS the current window).
///
/// `arg == None`, `b"$"` (last window), `b"#"` (previous window), and
/// the digit+direction form (e.g. `b"3j"`, via [`win_vert_neighbor`]/
/// [`win_horz_neighbor`]) are all modeled; any other unrecognized
/// `arg` returns `0` (matching the original's own `invalid_arg` path,
/// whose real `semsg` display is omitted - message display, not
/// tractable).
///
/// # Safety
/// `tp` must be a valid, non-null pointer to a live `TabpageT`, and
/// its own window list (`tp_firstwin`/`w_next`, or
/// `GLOBALS.firstwin`/`w_next` when `tp == GLOBALS.curtab`) must
/// consist of valid, live pointers.
#[must_use]
pub unsafe fn get_winnr(tp: *const crate::buffer_defs::TabpageT, arg: Option<&[u8]>) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let is_curtab = std::ptr::eq(tp, unsafe { crate::globals::GLOBALS.get_mut() }.curtab);
    let mut twin = if is_curtab {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::globals::GLOBALS.get_mut() }.curwin
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &*tp }.tp_curwin
    };

    let mut nr = 1;
    if let Some(arg) = arg {
        if arg == b"$" {
            twin = if is_curtab {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { crate::globals::GLOBALS.get_mut() }.lastwin
            } else {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { &*tp }.tp_lastwin
            };
        } else if arg == b"#" {
            twin = if is_curtab {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { crate::globals::GLOBALS.get_mut() }.prevwin
            } else {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { &*tp }.tp_prevwin
            };
            if twin.is_null() {
                nr = 0;
            }
        } else {
            let (count, consumed) = crate::charset::getdigits_int(arg, false, 0);
            let count = if count <= 0 { 1 } else { count };
            let dir = &arg[consumed..];
            let mut invalid_arg = false;
            if dir == b"j" {
                // SAFETY: forwarded from this function's own safety doc.
                twin = unsafe { win_vert_neighbor(tp, twin, false, count) };
            } else if dir == b"k" {
                // SAFETY: forwarded from this function's own safety doc.
                twin = unsafe { win_vert_neighbor(tp, twin, true, count) };
            } else if dir == b"h" {
                // SAFETY: forwarded from this function's own safety doc.
                twin = unsafe { win_horz_neighbor(tp, twin, true, count) };
            } else if dir == b"l" {
                // SAFETY: forwarded from this function's own safety doc.
                twin = unsafe { win_horz_neighbor(tp, twin, false, count) };
            } else {
                invalid_arg = true;
            }
            if invalid_arg {
                nr = 0;
            }
        }
    // SAFETY: forwarded from this function's own safety doc.
    } else if !unsafe { win_has_winnr(twin, tp) } {
        nr = 0;
    }

    if nr <= 0 {
        return 0;
    }

    nr = 0;
    let mut wp = if is_curtab {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::globals::GLOBALS.get_mut() }.firstwin
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &*tp }.tp_firstwin
    };
    while !wp.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        nr += i32::from(unsafe { win_has_winnr(wp, tp) });
        if std::ptr::eq(wp, twin) {
            break;
        }
        // SAFETY: forwarded from this function's own safety doc.
        wp = unsafe { &*wp }.w_next;
    }
    if wp.is_null() {
        nr = 0;
    }
    nr
}

/// Return the width, in columns, of `wp`'s `'foldcolumn'`
/// (`win_fdccol_count`).
///
/// `'foldcolumn'` is either a plain digit (`"0"`..`"9"`), used as-is,
/// or `"auto"`/`"auto:N"`, which caps the requested width (`N`, or
/// `1` for a bare `"auto"`) at the depth actually needed by the
/// window's fold nesting.
///
/// # Safety
/// For the `"auto"` forms this reaches
/// [`crate::fold::get_deepest_nesting`], so `wp` must satisfy that
/// function's own requirements (in particular a valid `w_buffer`,
/// since recomputing invalid folds reads the buffer).
#[must_use]
pub unsafe fn win_fdccol_count(wp: &mut WinT) -> i32 {
    let fdc = wp.w_onebuf_opt.wo_fdc.clone().unwrap_or_else(|| b"0".to_vec());

    // auto:<NUM>
    if fdc.starts_with(b"auto") {
        let fdccol = if fdc.get(4) == Some(&b':') {
            i32::from(fdc.get(5).copied().unwrap_or(b'0')) - i32::from(b'0')
        } else {
            1
        };
        // SAFETY: forwarded from this function's own safety doc.
        let needed_fdccols = unsafe { crate::fold::get_deepest_nesting(wp) };
        return fdccol.min(needed_fdccols);
    }

    i32::from(fdc.first().copied().unwrap_or(b'0')) - i32::from(b'0')
}

/// Check if the current window is allowed to move to a different
/// buffer (`check_can_set_curbuf_disabled`).
///
/// @return `false` if the window has `'winfixbuf'` set, `true`
/// otherwise.
///
/// Omits the original's real
/// `emsg(_(e_winfixbuf_cannot_go_to_buffer))` call - matching the
/// established "skip the deferred-subsystem side effect, keep the
/// state/return value correct" policy used throughout this crate.
///
/// # Safety
/// `crate::globals::GLOBALS.curwin` must be a valid, non-null pointer
/// to a live `WinT`.
#[must_use]
pub unsafe fn check_can_set_curbuf_disabled() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { &*crate::globals::GLOBALS.get_mut().curwin };
    curwin.w_onebuf_opt.wo_wfb == 0
}

/// Check if the current window is allowed to move to a different
/// buffer (`check_can_set_curbuf_forceit`).
///
/// @param forceit if `true`, always allowed. If `false` and
/// `'winfixbuf'` is enabled, not allowed.
///
/// Omits the original's real `emsg` call, matching
/// [`check_can_set_curbuf_disabled`].
///
/// # Safety
/// Same as [`check_can_set_curbuf_disabled`].
#[must_use]
pub unsafe fn check_can_set_curbuf_forceit(forceit: bool) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { &*crate::globals::GLOBALS.get_mut().curwin };
    forceit || curwin.w_onebuf_opt.wo_wfb == 0
}

/// Check that `tpc` points to a valid tab page (`valid_tabpage`).
///
/// # Safety
/// `crate::globals::GLOBALS.first_tabpage`'s own `tp_next` chain must
/// consist of valid, live `TabpageT` pointers (matching this crate's
/// usual global-linked-list-walk requirement, e.g. `undo.rs`'s
/// `any_buf_is_changed`).
#[must_use]
pub unsafe fn valid_tabpage(tpc: *const crate::buffer_defs::TabpageT) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let mut tp = unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage;
    while !tp.is_null() {
        if std::ptr::eq(tp, tpc) {
            return true;
        }
        // SAFETY: forwarded from this function's own safety doc.
        tp = unsafe { &*tp }.tp_next;
    }
    false
}

/// Get the 1-based index of tab page `ftp` (`tabpage_index`). When
/// `ftp` is not found in the list (including `ftp` being null, which
/// never matches any real tab page), returns the total number of tab
/// pages plus one - matching the original's own documented contract
/// exactly (used by `tabpagenr("$")`'s own `tabpage_index(NULL) - 1`
/// idiom to get a plain tab page COUNT).
///
/// # Safety
/// Same as [`valid_tabpage`].
#[must_use]
pub unsafe fn tabpage_index(ftp: *const crate::buffer_defs::TabpageT) -> i32 {
    let mut i = 1;
    // SAFETY: forwarded from this function's own safety doc.
    let mut tp = unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage;
    while !tp.is_null() && !std::ptr::eq(tp, ftp) {
        i += 1;
        // SAFETY: forwarded from this function's own safety doc.
        tp = unsafe { &*tp }.tp_next;
    }
    i
}

/// Find tab page number `n` (first one is `1`), or the current tab
/// page when `n == 0`. Returns a null pointer when not found
/// (`find_tabpage`).
///
/// # Safety
/// Same as [`valid_tabpage`].
#[must_use]
pub unsafe fn find_tabpage(n: i32) -> *mut crate::buffer_defs::TabpageT {
    if n == 0 {
        // SAFETY: forwarded from this function's own safety doc.
        return unsafe { crate::globals::GLOBALS.get_mut() }.curtab;
    }
    let mut i = 1;
    // SAFETY: forwarded from this function's own safety doc.
    let mut tp = unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage;
    while !tp.is_null() && i != n {
        i += 1;
        // SAFETY: forwarded from this function's own safety doc.
        tp = unsafe { &*tp }.tp_next;
    }
    tp
}

/// Find tab page `handle` in `GLOBALS.first_tabpage`'s own `tp_next`
/// list, or a null pointer if not found (`handle_get_tabpage`,
/// `api/private/helpers.h`). A plain `pmap_get(int)(&tabpage_handles,
/// (h))` in the original - this crate has no such registry, so this
/// walks the real tabpage list directly instead, matching
/// `handle_get_window`/`handle_get_buffer`'s own already-established
/// treatment of the identical macro family. Unlike [`find_tabpage`]
/// (which finds by 1-based POSITION), this finds by `TabpageT.handle`,
/// genuinely different lookups the original itself keeps separate too
/// (`find_tabpage`'s own real callers, e.g. `tabpagenr()`'s digit
/// argument, never go through `handle_get_tabpage` at all).
///
/// # Safety
/// Same as [`valid_tabpage`].
#[must_use]
pub unsafe fn handle_get_tabpage(
    handle: crate::types_defs::HandleT,
) -> *mut crate::buffer_defs::TabpageT {
    // SAFETY: forwarded from this function's own safety doc.
    let mut tp = unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage;
    while !tp.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { &*tp }.handle == handle {
            return tp;
        }
        // SAFETY: forwarded from this function's own safety doc.
        tp = unsafe { &*tp }.tp_next;
    }
    std::ptr::null_mut()
}

/// Lowest real window handle used as a genuine window ID rather than
/// a window NUMBER within the current tab page (`LOWEST_WIN_ID`,
/// `window.h`).
pub const LOWEST_WIN_ID: i32 = 1000;

/// Find window number `nr` in tabpage `tp` (`NULL` meaning `curtab`)
/// (`find_win_by_nr`, `eval/window.c`). `nr == 0` means `curwin`;
/// `nr >= `[`LOWEST_WIN_ID`] is treated as a real window ID (handle)
/// instead of a plain window number.
///
/// # Safety
/// `tp` (if non-null) must be a valid, live `TabpageT` pointer, and
/// its own `tp_firstwin`/`w_next` chain must consist of valid, live
/// pointers - same for `GLOBALS.firstwin`/`curtab`/`curwin` when `tp`
/// is null.
#[must_use]
pub unsafe fn find_win_by_nr(vp: &TypvalT, tp: *mut crate::buffer_defs::TabpageT) -> *mut WinT {
    let mut error = false;
    let mut nr = crate::eval::typval::tv_get_number_chk(vp, Some(&mut error));
    if error || nr < 0 {
        return std::ptr::null_mut();
    }
    if nr == 0 {
        // SAFETY: forwarded from this function's own safety doc.
        return unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
    }

    // This method accepts NULL as an alias for curtab.
    // SAFETY: forwarded from this function's own safety doc.
    let tp = if tp.is_null() { unsafe { crate::globals::GLOBALS.get_mut() }.curtab } else { tp };

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
        let handle = i64::from(unsafe { &*wp }.handle);
        if nr >= i64::from(LOWEST_WIN_ID) {
            if handle == nr {
                return wp;
            }
        } else {
            nr -= 1;
            if nr <= 0 {
                return wp;
            }
        }
        // SAFETY: forwarded from this function's own safety doc.
        wp = unsafe { &*wp }.w_next;
    }
    std::ptr::null_mut()
}

/// Return the window and tab pointer of window handle `id`, `NULL`
/// when not found (`win_id2wp_tp`, `eval/window.c`).
///
/// # Safety
/// `GLOBALS.first_tabpage`'s own `tp_next` chain, and each tabpage's
/// own `tp_firstwin`/`w_next` chain, must consist of valid, live
/// pointers.
#[must_use]
pub unsafe fn win_id2wp_tp(id: i32) -> (*mut WinT, *mut crate::buffer_defs::TabpageT) {
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
            if unsafe { &*wp }.handle == id {
                return (wp, tp);
            }
            // SAFETY: forwarded from this function's own safety doc.
            wp = unsafe { &*wp }.w_next;
        }
        // SAFETY: forwarded from this function's own safety doc.
        tp = unsafe { &*tp }.tp_next;
    }
    (std::ptr::null_mut(), std::ptr::null_mut())
}

/// [`win_id2wp_tp`] without the tabpage out-value (`win_id2wp`,
/// `eval/window.c`).
///
/// # Safety
/// Forwarded from [`win_id2wp_tp`]'s own safety doc.
#[must_use]
pub unsafe fn win_id2wp(id: i32) -> *mut WinT {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { win_id2wp_tp(id) }.0
}

/// Find a window: using a Window ID in any tab page, or using a
/// number in the current tab page (`find_win_by_nr_or_id`,
/// `eval/window.c`).
///
/// # Safety
/// Forwarded from [`win_id2wp`]/[`find_win_by_nr`]'s own safety docs.
#[must_use]
pub unsafe fn find_win_by_nr_or_id(vp: &TypvalT) -> *mut WinT {
    let nr = crate::eval::typval::tv_get_number_chk(vp, None);
    if nr >= i64::from(LOWEST_WIN_ID) {
        // SAFETY: forwarded from this function's own safety doc.
        return unsafe { win_id2wp(crate::eval::typval::tv_get_number(vp) as i32) };
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { find_win_by_nr(vp, std::ptr::null_mut()) }
}

/// Find a window given by `wvp` (a window number or ID, `Unknown`
/// meaning `curwin`) within tabpage `tvp` (a tab number, `Unknown`
/// meaning `curtab`) (`find_tabwin`, `eval/window.c`).
///
/// # Safety
/// Forwarded from [`find_tabpage`]/[`find_win_by_nr`]'s own safety
/// docs.
#[must_use]
pub unsafe fn find_tabwin(wvp: &TypvalT, tvp: &TypvalT) -> *mut WinT {
    if matches!(wvp.value, TypvalValue::Unknown) {
        // SAFETY: forwarded from this function's own safety doc.
        return unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
    }

    let tp = if matches!(tvp.value, TypvalValue::Unknown) {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::globals::GLOBALS.get_mut() }.curtab
    } else {
        let n = crate::eval::typval::tv_get_number(tvp) as i32;
        if n >= 0 {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { find_tabpage(n) }
        } else {
            std::ptr::null_mut()
        }
    };

    if tp.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { find_win_by_nr(wvp, tp) }
}

/// Get the leaf window contained within frame `frp` (`frame2win`) - a
/// non-leaf frame's own `fr_child` chain always bottoms out at a leaf
/// (`fr_win.is_some()`) eventually, matching the original's own
/// unconditional `while (frp->fr_win == NULL) { frp = frp->fr_child; }`
/// loop (no bounds/null check - a well-formed frame tree, which is all
/// this crate can ever construct, always terminates).
///
/// # Safety
/// `frp` must be a valid, non-null pointer to a live `FrameT`, and its
/// own `fr_child` chain must consist of valid, live `FrameT` pointers
/// down to a leaf.
#[must_use]
pub unsafe fn frame2win(mut frp: *const crate::buffer_defs::FrameT) -> *mut WinT {
    loop {
        // SAFETY: forwarded from this function's own safety doc.
        let fr = unsafe { &*frp };
        if !fr.fr_win.is_null() {
            return fr.fr_win;
        }
        frp = fr.fr_child;
    }
}

/// Return `true` if the height of frame `frp` should not be changed
/// because of `'winfixheight'` (`frame_fixed_height`). A leaf frame is
/// fixed height exactly when its own window's `'winfixheight'` is
/// set; a `FR_ROW` (side-by-side) frame is fixed height if ANY child
/// is; a `FR_COL` (stacked) frame is fixed height only if ALL children
/// are - matching the original's own `FOR_ALL_FRAMES` walk over
/// `fr_child`/`fr_next` exactly (translated as 2 separate, explicit
/// loops rather than one parameterized "any vs all" loop, matching
/// the original's own 2-branch structure directly rather than a
/// cleverer-but-less-obviously-correct consolidation).
///
/// # Safety
/// `frp` must be a valid, non-null pointer to a live `FrameT`, whose
/// own `fr_child`/`fr_next` chain (if any) consists entirely of valid,
/// live `FrameT` pointers, and whose `fr_win` (if non-null) is a
/// valid, live `WinT` pointer.
#[must_use]
pub unsafe fn frame_fixed_height(frp: *const crate::buffer_defs::FrameT) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let fr = unsafe { &*frp };
    if !fr.fr_win.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        return unsafe { &*fr.fr_win }.w_onebuf_opt.wo_wfh != 0;
    }
    if fr.fr_layout == crate::buffer_defs::FR_ROW {
        // Fixed height if ONE of the frames in the row is fixed height.
        let mut child = fr.fr_child;
        while !child.is_null() {
            // SAFETY: forwarded from this function's own safety doc.
            if unsafe { frame_fixed_height(child) } {
                return true;
            }
            // SAFETY: forwarded from this function's own safety doc.
            child = unsafe { &*child }.fr_next;
        }
        return false;
    }
    // fr.fr_layout == FR_COL: fixed height if ALL of the frames in the
    // column are fixed height.
    let mut child = fr.fr_child;
    while !child.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        if !unsafe { frame_fixed_height(child) } {
            return false;
        }
        // SAFETY: forwarded from this function's own safety doc.
        child = unsafe { &*child }.fr_next;
    }
    true
}

/// Return `true` if the width of frame `frp` should not be changed
/// because of `'winfixwidth'` (`frame_fixed_width`) - the `FR_COL`/
/// `FR_ROW` "any"/"all" roles are swapped relative to
/// [`frame_fixed_height`] (a `FR_COL` frame is fixed width if ANY
/// child is; a `FR_ROW` frame only if ALL are), matching the
/// original's own exact structure.
///
/// # Safety
/// Same as [`frame_fixed_height`].
#[must_use]
pub unsafe fn frame_fixed_width(frp: *const crate::buffer_defs::FrameT) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let fr = unsafe { &*frp };
    if !fr.fr_win.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        return unsafe { &*fr.fr_win }.w_onebuf_opt.wo_wfw != 0;
    }
    if fr.fr_layout == crate::buffer_defs::FR_COL {
        // Fixed width if ONE of the frames in the column is fixed width.
        let mut child = fr.fr_child;
        while !child.is_null() {
            // SAFETY: forwarded from this function's own safety doc.
            if unsafe { frame_fixed_width(child) } {
                return true;
            }
            // SAFETY: forwarded from this function's own safety doc.
            child = unsafe { &*child }.fr_next;
        }
        return false;
    }
    // fr.fr_layout == FR_ROW: fixed width if ALL of the frames in the
    // row are fixed width.
    let mut child = fr.fr_child;
    while !child.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        if !unsafe { frame_fixed_width(child) } {
            return false;
        }
        // SAFETY: forwarded from this function's own safety doc.
        child = unsafe { &*child }.fr_next;
    }
    true
}

/// Set frame width from the window it contains (`frame_fix_width`).
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT` whose own
/// `w_frame` is a valid, non-null pointer to a live `FrameT`.
pub unsafe fn frame_fix_width(wp: *mut WinT) {
    // SAFETY: forwarded from this function's own safety doc.
    let win = unsafe { &*wp };
    let width = win.w_width + win.w_vsep_width;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { &mut *win.w_frame }.fr_width = width;
}

/// Set frame height from the window it contains (`frame_fix_height`).
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT` whose own
/// `w_frame` is a valid, non-null pointer to a live `FrameT`.
pub unsafe fn frame_fix_height(wp: *mut WinT) {
    // SAFETY: forwarded from this function's own safety doc.
    let win = unsafe { &*wp };
    let height = win.w_height + win.w_hsep_height + win.w_status_height;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { &mut *win.w_frame }.fr_height = height;
}

/// Sentinel value for [`frame_minheight`]/[`frame_minwidth`]'s own
/// `next_curwin` parameter, meaning "don't reserve at least one line/
/// column for the current window" (`NOWIN`, `((win_T *)-1)` in the
/// original - a real, non-null, but deliberately invalid pointer
/// value, distinguished from both a null pointer, the ordinary case,
/// and any genuine `*mut WinT`).
pub const NOWIN: *mut WinT = -1isize as *mut WinT;

/// Compute the minimal height for frame `topfrp` (`frame_minheight`),
/// using `'winminheight'`. When `next_curwin` is a real window
/// pointer, uses `'winheight'` for THAT window instead. When
/// `next_curwin` is [`NOWIN`], don't reserve at least one line for
/// the current window (`GLOBALS.curwin`).
///
/// # Safety
/// `topfrp` must be a valid, non-null pointer to a live `FrameT`,
/// whose own `fr_child`/`fr_next` chain (if any) consists entirely of
/// valid, live `FrameT` pointers, and whose `fr_win` (if non-null) is
/// a valid, live `WinT` pointer. `next_curwin` must be either
/// [`NOWIN`], null, or a valid, live `WinT` pointer. Touches
/// `crate::globals::GLOBALS`/`crate::option_vars::OPTION_VARS`.
#[must_use]
pub unsafe fn frame_minheight(topfrp: *const crate::buffer_defs::FrameT, next_curwin: *mut WinT) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let fr = unsafe { &*topfrp };
    if !fr.fr_win.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        let win = unsafe { &*fr.fr_win };
        // Combined height of window bar and separator column or status line.
        let extra_height = win.w_winbar_height + win.w_hsep_height + win.w_status_height;
        // SAFETY: forwarded from this function's own safety doc.
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        if std::ptr::eq(fr.fr_win, next_curwin) {
            opts.p_wh as i32 + extra_height
        } else {
            let mut m = opts.p_wmh as i32 + extra_height;
            // SAFETY: forwarded from this function's own safety doc.
            let curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
            if std::ptr::eq(fr.fr_win, curwin as *const WinT) && next_curwin.is_null() {
                // Current window is minimal one line high.
                if opts.p_wmh == 0 {
                    m += 1;
                }
            }
            m
        }
    } else if fr.fr_layout == crate::buffer_defs::FR_ROW {
        // get the minimal height from each frame in this row
        let mut m = 0;
        let mut child = fr.fr_child;
        while !child.is_null() {
            // SAFETY: forwarded from this function's own safety doc.
            let n = unsafe { frame_minheight(child, next_curwin) };
            if n > m {
                m = n;
            }
            // SAFETY: forwarded from this function's own safety doc.
            child = unsafe { &*child }.fr_next;
        }
        m
    } else {
        // Add up the minimal heights for all frames in this column.
        let mut m = 0;
        let mut child = fr.fr_child;
        while !child.is_null() {
            // SAFETY: forwarded from this function's own safety doc.
            m += unsafe { frame_minheight(child, next_curwin) };
            // SAFETY: forwarded from this function's own safety doc.
            child = unsafe { &*child }.fr_next;
        }
        m
    }
}

/// Compute the minimal width for frame `topfrp` (`frame_minwidth`),
/// using `'winminwidth'`. When `next_curwin` is a real window
/// pointer, uses `'winwidth'` for THAT window instead. When
/// `next_curwin` is [`NOWIN`], don't reserve at least one column for
/// the current window (`GLOBALS.curwin`).
///
/// # Safety
/// Same as [`frame_minheight`].
#[must_use]
pub unsafe fn frame_minwidth(topfrp: *const crate::buffer_defs::FrameT, next_curwin: *mut WinT) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let fr = unsafe { &*topfrp };
    if !fr.fr_win.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        let win = unsafe { &*fr.fr_win };
        // SAFETY: forwarded from this function's own safety doc.
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        if std::ptr::eq(fr.fr_win, next_curwin) {
            opts.p_wiw as i32 + win.w_vsep_width
        } else {
            // window: minimal width of the window plus separator column
            let mut m = opts.p_wmw as i32 + win.w_vsep_width;
            // SAFETY: forwarded from this function's own safety doc.
            let curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
            // Current window is minimal one column wide.
            if opts.p_wmw == 0 && std::ptr::eq(fr.fr_win, curwin as *const WinT) && next_curwin.is_null() {
                m += 1;
            }
            m
        }
    } else if fr.fr_layout == crate::buffer_defs::FR_COL {
        // get the minimal width from each frame in this column
        let mut m = 0;
        let mut child = fr.fr_child;
        while !child.is_null() {
            // SAFETY: forwarded from this function's own safety doc.
            let n = unsafe { frame_minwidth(child, next_curwin) };
            m = m.max(n);
            // SAFETY: forwarded from this function's own safety doc.
            child = unsafe { &*child }.fr_next;
        }
        m
    } else {
        // Add up the minimal widths for all frames in this row.
        let mut m = 0;
        let mut child = fr.fr_child;
        while !child.is_null() {
            // SAFETY: forwarded from this function's own safety doc.
            m += unsafe { frame_minwidth(child, next_curwin) };
            // SAFETY: forwarded from this function's own safety doc.
            child = unsafe { &*child }.fr_next;
        }
        m
    }
}

/// Return the default value for `'scroll'` for window `wp`
/// (`win_default_scroll`).
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT`.
#[must_use]
pub unsafe fn win_default_scroll(wp: *const WinT) -> crate::types_defs::OptInt {
    // SAFETY: forwarded from this function's own safety doc.
    let w_view_height = unsafe { &*wp }.w_view_height;
    crate::types_defs::OptInt::from((w_view_height / 2).max(1))
}

/// Return the number of lines used by the tab page line
/// (`tabline_height`), via `'showtabline'`.
///
/// # Safety
/// Touches `crate::globals::GLOBALS`/`crate::option_vars::OPTION_VARS`.
#[must_use]
pub unsafe fn tabline_height() -> i32 {
    if crate::ui::ui_has(crate::ui::UiExtension::Tabline) {
        return 0;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let first_tabpage = unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage;
    debug_assert!(!first_tabpage.is_null());
    // SAFETY: forwarded from this function's own safety doc.
    let only_one_tab = unsafe { &*first_tabpage }.tp_next.is_null();
    // SAFETY: forwarded from this function's own safety doc.
    match unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_stal {
        0 => 0,
        1 if only_one_tab => 0,
        1 => 1,
        _ => 1,
    }
}

/// Return the number of lines used by the global statusline
/// (`global_stl_height`), via `'laststatus'`.
///
/// # Safety
/// Touches `crate::option_vars::OPTION_VARS`.
#[must_use]
pub unsafe fn global_stl_height() -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ls == 3 {
        STATUS_HEIGHT
    } else {
        0
    }
}

/// Remember every window's current scroll position and size, so a
/// later comparison can tell what moved
/// (`snapshot_windows_scroll_size`).
///
/// As elsewhere in this crate, the original's
/// `FOR_ALL_WINDOWS_IN_TAB(wp, curtab)` is walked as
/// `GLOBALS.firstwin`/`w_next`.
///
/// # Safety
/// `GLOBALS.firstwin`'s own `w_next` chain must consist of valid,
/// live `WinT` pointers.
pub unsafe fn snapshot_windows_scroll_size() {
    // SAFETY: forwarded from this function's own safety doc.
    let mut wp = unsafe { crate::globals::GLOBALS.get_mut() }.firstwin;
    while !wp.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        let next = unsafe { (*wp).w_next };
        // SAFETY: forwarded from this function's own safety doc.
        unsafe {
            (*wp).w_last_topline = (*wp).w_topline;
            (*wp).w_last_topfill = (*wp).w_topfill;
            (*wp).w_last_leftcol = (*wp).w_leftcol;
            (*wp).w_last_skipcol = (*wp).w_skipcol;
            (*wp).w_last_width = (*wp).w_width;
            (*wp).w_last_height = (*wp).w_height;
        };
        wp = next;
    }
}

/// Whether the initial scroll-size snapshot has been taken
/// (`did_initial_scroll_size_snapshot`, a `static` in the original).
static DID_INITIAL_SCROLL_SIZE_SNAPSHOT: std::sync::LazyLock<crate::globals::GlobalCell<bool>> =
    std::sync::LazyLock::new(|| crate::globals::GlobalCell::new(false));

/// Take the initial scroll-size snapshot, once
/// (`may_make_initial_scroll_size_snapshot`).
///
/// Guarded by a one-shot flag: only the FIRST call snapshots, so a
/// later call cannot overwrite the baseline the `WinScrolled`
/// autocommand compares against.
///
/// # Safety
/// Forwarded from [`snapshot_windows_scroll_size`]'s own safety doc;
/// also mutates the shared one-shot flag.
pub unsafe fn may_make_initial_scroll_size_snapshot() {
    // SAFETY: forwarded from this function's own safety doc.
    let done = unsafe { DID_INITIAL_SCROLL_SIZE_SNAPSHOT.get_mut() };
    if !*done {
        *done = true;
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { snapshot_windows_scroll_size() };
    }
}

/// Rows available to the window layout: everything except the command
/// line, the tab line and a global status line (`ROWS_AVAIL`, a macro
/// in the original).
///
/// # Safety
/// Forwarded from [`tabline_height`]/[`global_stl_height`]'s own
/// safety docs; also reads `GLOBALS.Rows`.
#[must_use]
pub unsafe fn rows_avail() -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let rows = unsafe { crate::globals::GLOBALS.get_mut() }.Rows;
    // SAFETY: as above.
    let p_ch = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ch as i32;
    // SAFETY: forwarded from this function's own safety doc.
    let tabline = unsafe { tabline_height() };
    // SAFETY: as above.
    let global_stl = unsafe { global_stl_height() };
    rows - p_ch - tabline - global_stl
}

/// Look for a frame that can give up a line, starting at `fr`
/// (`find_horizontally_resizable_frame`).
///
/// Walks upward while the current frame is already at its minimum
/// height, so the frame returned is the nearest ancestor (or preceding
/// sibling) with room to shrink. Returns null when even the top frame
/// is at its minimum, meaning nothing can be resized.
///
/// The walk direction depends on the parent's layout: inside a COLUMN
/// of frames it steps to the frame ABOVE, since that one shares the
/// vertical space; anywhere else (a row, or already at the top of a
/// column) it steps up to the parent.
///
/// # Safety
/// `fr` must be a valid, non-null pointer to a live frame whose own
/// `fr_parent`/`fr_prev` links are likewise valid or null, and whose
/// ancestry reaches `GLOBALS.topframe`. Forwarded from
/// [`frame_minheight`]'s own safety doc.
pub unsafe fn find_horizontally_resizable_frame(
    fr: *mut crate::buffer_defs::FrameT,
) -> *mut crate::buffer_defs::FrameT {
    let mut fp = fr;
    // SAFETY: forwarded from this function's own safety doc.
    let topframe = unsafe { crate::globals::GLOBALS.get_mut() }.topframe;

    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        while (*fp).fr_height <= frame_minheight(fp, std::ptr::null_mut()) {
            if fp == topframe {
                return std::ptr::null_mut();
            }
            // In a column of frames: go to the frame above. If already
            // at the top, or in a row of frames: go to the parent.
            if (*(*fp).fr_parent).fr_layout == crate::buffer_defs::FR_COL
                && !(*fp).fr_prev.is_null()
            {
                fp = (*fp).fr_prev;
            } else {
                fp = (*fp).fr_parent;
            }
        }
    }
    fp
}

/// Record every window's current size so it can be restored later
/// (`win_size_save`).
///
/// The FIRST entry is the total number of lines available for windows,
/// not a window size: a later restore compares it against the value at
/// that time and gives up if the screen has since been resized. Each
/// window then contributes two entries, width-plus-separator followed
/// by height.
///
/// The original fills a `garray_T` out-parameter sized ahead of time
/// via `ga_grow`; a returned `Vec<i32>` needs no pre-sizing and
/// carries its own length.
///
/// As elsewhere in this crate, `FOR_ALL_WINDOWS_IN_TAB(wp, curtab)` is
/// walked as `GLOBALS.firstwin`/`w_next`, the established
/// simplification here.
///
/// # Safety
/// `GLOBALS.firstwin`'s own `w_next` chain must consist of valid, live
/// `WinT` pointers. Forwarded from [`rows_avail`]/[`global_stl_height`]
/// /[`last_stl_height`]'s own safety docs.
#[must_use]
pub unsafe fn win_size_save() -> Vec<i32> {
    // SAFETY: forwarded from this function's own safety doc.
    let total = unsafe { rows_avail() + global_stl_height() - last_stl_height(false) };

    // First entry is the total lines available for windows.
    let mut gap = vec![total];

    // SAFETY: forwarded from this function's own safety doc.
    let mut wp = unsafe { crate::globals::GLOBALS.get_mut() }.firstwin;
    while !wp.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe {
            gap.push((*wp).w_width + (*wp).w_vsep_width);
            gap.push((*wp).w_height);
            wp = (*wp).w_next;
        }
    }
    gap
}

/// Recursively copy a frame tree's layout into a fresh snapshot
/// (`make_snapshot_rec`).
///
/// Only the layout shape and sizes are recorded, NOT the windows
/// themselves - with one deliberate exception: the leaf holding
/// `curwin` stores that pointer, so the snapshot can later restore
/// which window was current. Every other leaf's `fr_win` stays null,
/// which is what [`get_snapshot_curwin_rec`]'s fallthrough relies on.
///
/// # Safety
/// `fr` must be a valid, non-null pointer to a live frame whose own
/// `fr_next`/`fr_child` links are likewise valid or null. The returned
/// tree is heap-allocated and must eventually be freed with
/// [`clear_snapshot_rec`].
pub unsafe fn make_snapshot_rec(fr: *mut crate::buffer_defs::FrameT) -> *mut crate::buffer_defs::FrameT {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        let curwin = crate::globals::GLOBALS.get_mut().curwin;
        let mut snap = Box::new(crate::buffer_defs::FrameT {
            fr_layout: (*fr).fr_layout,
            fr_width: (*fr).fr_width,
            fr_height: (*fr).fr_height,
            ..Default::default()
        });
        if !(*fr).fr_next.is_null() {
            snap.fr_next = make_snapshot_rec((*fr).fr_next);
        }
        if !(*fr).fr_child.is_null() {
            snap.fr_child = make_snapshot_rec((*fr).fr_child);
        }
        if (*fr).fr_layout == crate::buffer_defs::FR_LEAF && (*fr).fr_win == curwin {
            snap.fr_win = curwin;
        }
        Box::into_raw(snap)
    }
}

/// Whether snapshot `sn` still matches the live frame tree `fr`
/// (`check_snapshot_rec`).
///
/// The layout must match structurally - same kind, and the same
/// presence-or-absence of a sibling and a child at every level - and
/// any window the snapshot recorded must still be valid. A window that
/// has since been closed invalidates the whole snapshot, since
/// restoring it would install a dangling pointer.
///
/// @return [`crate::vim_defs::OK`]/[`crate::vim_defs::FAIL`], matching
///         the original.
///
/// # Safety
/// Both `sn` and `fr` must be valid, non-null pointers to live frames
/// whose own links are likewise valid or null. Forwarded from
/// [`win_valid`]'s own safety doc.
pub unsafe fn check_snapshot_rec(
    sn: *mut crate::buffer_defs::FrameT,
    fr: *mut crate::buffer_defs::FrameT,
) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        if (*sn).fr_layout != (*fr).fr_layout
            || (*sn).fr_next.is_null() != (*fr).fr_next.is_null()
            || (*sn).fr_child.is_null() != (*fr).fr_child.is_null()
            || (!(*sn).fr_next.is_null()
                && check_snapshot_rec((*sn).fr_next, (*fr).fr_next) == crate::vim_defs::FAIL)
            || (!(*sn).fr_child.is_null()
                && check_snapshot_rec((*sn).fr_child, (*fr).fr_child) == crate::vim_defs::FAIL)
            || (!(*sn).fr_win.is_null() && !win_valid((*sn).fr_win))
        {
            return crate::vim_defs::FAIL;
        }
        crate::vim_defs::OK
    }
}

/// Free the snapshot at `idx` in tab page `tp` and clear its slot
/// (`clear_snapshot`).
///
/// Clearing the slot is what makes this safe to call twice: the
/// recursive free would otherwise walk an already-freed tree.
///
/// # Safety
/// `tp` must be a valid, non-null pointer to a live `TabpageT`, and
/// `idx` must be within `SNAP_COUNT`. Forwarded from
/// [`clear_snapshot_rec`]'s own safety doc.
pub unsafe fn clear_snapshot(tp: *mut crate::buffer_defs::TabpageT, idx: usize) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        clear_snapshot_rec((*tp).tp_snapshot[idx]);
        (*tp).tp_snapshot[idx] = std::ptr::null_mut();
    }
}

/// The window that was current when snapshot `idx` was taken, or null
/// (`get_snapshot_curwin`).
///
/// # Safety
/// `GLOBALS.curtab` must be valid and non-null, and `idx` must be
/// within `SNAP_COUNT`. Forwarded from [`get_snapshot_curwin_rec`]'s
/// own safety doc.
pub unsafe fn get_snapshot_curwin(idx: usize) -> *mut WinT {
    // SAFETY: forwarded from this function's own safety doc.
    let curtab = unsafe { crate::globals::GLOBALS.get_mut() }.curtab;
    // SAFETY: forwarded from this function's own safety doc.
    let snap = unsafe { (*curtab).tp_snapshot[idx] };
    if snap.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { get_snapshot_curwin_rec(snap) }
}

/// Free a snapshot frame tree (`clear_snapshot_rec`).
///
/// Both the sibling chain and the child subtree are freed before the
/// node itself, so no pointer is read after its owner is gone.
///
/// # Safety
/// `fr` must be either null or a valid pointer to a frame tree whose
/// nodes were each allocated as a `Box<FrameT>` (i.e. every
/// `fr_next`/`fr_child` link must likewise be null or such an
/// allocation). Every node in the tree is freed, so no pointer into it
/// may be used afterwards.
pub unsafe fn clear_snapshot_rec(fr: *mut crate::buffer_defs::FrameT) {
    if fr.is_null() {
        return;
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        clear_snapshot_rec((*fr).fr_next);
        clear_snapshot_rec((*fr).fr_child);
        // Reclaims the Box allocation this node was created with.
        drop(Box::from_raw(fr));
    }
}

/// Traverse a snapshot to find the previous `curwin`
/// (`get_snapshot_curwin_rec`).
///
/// Siblings are searched before children, and the node's own `fr_win`
/// is only used when neither yielded a window - so the DEEPEST,
/// last-visited branch wins. A leaf frame's `fr_win` is the window it
/// holds; an interior frame's is null, which is exactly what makes the
/// "keep looking" fallthrough work.
///
/// # Safety
/// `ft` must be a valid, non-null pointer to a live snapshot frame
/// whose own `fr_next`/`fr_child` links are likewise valid or null.
pub unsafe fn get_snapshot_curwin_rec(ft: *mut crate::buffer_defs::FrameT) -> *mut WinT {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        if !(*ft).fr_next.is_null() {
            let wp = get_snapshot_curwin_rec((*ft).fr_next);
            if !wp.is_null() {
                return wp;
            }
        }
        if !(*ft).fr_child.is_null() {
            let wp = get_snapshot_curwin_rec((*ft).fr_child);
            if !wp.is_null() {
                return wp;
            }
        }
        (*ft).fr_win
    }
}

/// Remove `wp`'s status line (`win_remove_status_line`).
///
/// The freed height goes back to the window itself, unless a
/// horizontal separator takes the status line's place instead.
///
/// # Safety
/// `wp` must point at a live `WinT`. Forwards
/// [`win_new_height`]/[`crate::drawscreen::comp_col`]'s own safety
/// docs.
pub unsafe fn win_remove_status_line(wp: *mut WinT, add_hsep: bool) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*wp).w_status_height = 0 };

    if add_hsep {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*wp).w_hsep_height = 1 };
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        let height = unsafe {
            if (*wp).w_floating { (*wp).w_view_height } else { (*wp).w_height }
        };
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { win_new_height(wp, height + STATUS_HEIGHT) };
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::drawscreen::comp_col() };

    // SAFETY: forwarded from this function's own safety doc.
    let click_defs = unsafe { &mut (*wp).w_status_click_defs };
    crate::statusline::stl_clear_click_defs(click_defs);
    // The original frees the array and zeroes its separate size field;
    // this crate folds both into one `Vec`, so clearing it is the
    // whole of that.
    click_defs.clear();
}

/// Initialise the current window, when a new file is being edited
/// (`curwin_init`).
///
/// # Safety
/// `GLOBALS.curwin` must point at a live `WinT`, and
/// [`win_init_empty`]'s own safety doc applies.
pub unsafe fn curwin_init() {
    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { win_init_empty(curwin) };
}

/// Reset a window to the "empty buffer" starting state
/// (`win_init_empty`).
///
/// Called when a window is given a fresh buffer: every cached view
/// position goes back to the top, and `w_valid` is cleared so nothing
/// stale is trusted.
///
/// Note `w_pcmark` is set to line 1 rather than cleared - the
/// original's own comment says so explicitly - while `w_prev_pcmark`
/// IS cleared to line 0. The asymmetry is deliberate.
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT` whose own
/// `w_buffer` is likewise valid and non-null, for the whole call.
/// Forwarded from [`crate::drawscreen::redraw_later`]'s own safety doc.
pub unsafe fn win_init_empty(wp: *mut WinT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::drawscreen::redraw_later(wp, crate::drawscreen::UPD_NOT_VALID) };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        (*wp).w_lines_valid = 0;
        (*wp).w_cursor.lnum = 1;
        (*wp).w_cursor.col = 0;
        (*wp).w_curswant = 0;
        (*wp).w_cursor.coladd = 0;
        // pcmark not cleared but set to line 1.
        (*wp).w_pcmark.lnum = 1;
        (*wp).w_pcmark.col = 0;
        (*wp).w_prev_pcmark.lnum = 0;
        (*wp).w_prev_pcmark.col = 0;
        (*wp).w_topline = 1;
        (*wp).w_topfill = 0;
        (*wp).w_botline = 2;
        (*wp).w_valid = 0;
        (*wp).w_s = &raw mut (*(*wp).w_buffer).b_s;
    }
}

/// Give `wp` a fresh leaf frame of its own (`new_frame`).
///
/// The frame is heap-allocated and ownership handed to `wp.w_frame`,
/// matching the original's `xcalloc`; the window's own teardown frees
/// it again.
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT` for the
/// whole call. Any frame already in `wp.w_frame` is overwritten
/// without being freed - the original does the same, relying on this
/// only ever being called for a window that has none yet.
pub unsafe fn new_frame(wp: *mut WinT) {
    let frp = Box::into_raw(Box::new(crate::buffer_defs::FrameT {
        fr_layout: crate::buffer_defs::FR_LEAF,
        fr_win: wp,
        ..Default::default()
    }));
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*wp).w_frame = frp };
}

/// Size the first window and the top frame to fill the screen
/// (`win_init_size`), used when the layout is a single window.
///
/// # Safety
/// `GLOBALS.firstwin` and `GLOBALS.topframe` must be valid, non-null
/// pointers to live values. Forwarded from [`rows_avail`]'s own
/// safety doc.
pub unsafe fn win_init_size() {
    // SAFETY: forwarded from this function's own safety doc.
    let avail = unsafe { rows_avail() };
    // SAFETY: as above.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    let columns = g.Columns;
    let (firstwin, topframe) = (g.firstwin, g.topframe);

    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        (*firstwin).w_height = avail;
        (*firstwin).w_prev_height = avail;
        (*firstwin).w_view_height = avail - (*firstwin).w_winbar_height;
        (*firstwin).w_height_outer = avail;
        (*firstwin).w_winrow_off = (*firstwin).w_winbar_height;
        (*topframe).fr_height = avail;
        (*firstwin).w_width = columns;
        (*firstwin).w_view_width = columns;
        (*firstwin).w_width_outer = columns;
        (*topframe).fr_width = columns;
    };
}

/// The window whose buffer should be treated as the "previous" one
/// (`prevwin_curwin`).
///
/// Normally the current window, but inside the command-line window
/// the alternative buffer belongs to `prevwin` instead.
///
/// # Safety
/// Forwarded from [`crate::ex_getln::is_in_cmdwin`]'s own safety doc;
/// also reads `GLOBALS.prevwin`/`curwin`.
#[must_use]
pub unsafe fn prevwin_curwin() -> *mut WinT {
    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    // In cmdwin, the alternative buffer should be used.
    // SAFETY: as above.
    if unsafe { crate::ex_getln::is_in_cmdwin() } && !g.prevwin.is_null() {
        g.prevwin
    } else {
        g.curwin
    }
}

/// Return the minimal number of rows needed on the screen to display
/// the current number of windows for tab page `tp` (`min_rows`).
///
/// # Safety
/// `tp` must be a valid, non-null pointer to a live `TabpageT`, whose
/// own `tp_topframe` frame tree consists of valid, live pointers.
/// Touches `crate::globals::GLOBALS`/`crate::option_vars::OPTION_VARS`.
#[must_use]
pub unsafe fn min_rows(tp: *const crate::buffer_defs::TabpageT) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let globals = unsafe { crate::globals::GLOBALS.get_mut() };
    if globals.firstwin.is_null() {
        // not initialized yet
        return MIN_LINES;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let t = unsafe { &*tp };
    // SAFETY: forwarded from this function's own safety doc.
    let mut total = unsafe { frame_minheight(t.tp_topframe, std::ptr::null_mut()) };
    // SAFETY: forwarded from this function's own safety doc.
    total += unsafe { tabline_height() } + unsafe { global_stl_height() };
    let ch_used = if std::ptr::eq(tp, globals.curtab) {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ch
    } else {
        t.tp_ch_used
    };
    if ch_used > 0 {
        total += 1; // count the room for the command line
    }
    total
}

/// Return the minimal number of rows needed on the screen to display
/// the current number of windows for ALL tab pages
/// (`min_rows_for_all_tabpages`).
///
/// # Safety
/// `crate::globals::GLOBALS.first_tabpage`'s own `tp_next` chain must
/// consist of valid, live `TabpageT` pointers, each with a valid,
/// live `tp_topframe` frame tree. Touches
/// `crate::globals::GLOBALS`/`crate::option_vars::OPTION_VARS`.
#[must_use]
pub unsafe fn min_rows_for_all_tabpages() -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let globals = unsafe { crate::globals::GLOBALS.get_mut() };
    if globals.firstwin.is_null() {
        // not initialized yet
        return MIN_LINES;
    }

    let mut total = 0;
    let mut tp = globals.first_tabpage;
    while !tp.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        let t = unsafe { &*tp };
        // SAFETY: forwarded from this function's own safety doc.
        let mut n = unsafe { frame_minheight(t.tp_topframe, std::ptr::null_mut()) };
        let ch_used = if std::ptr::eq(tp, globals.curtab) {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ch
        } else {
            t.tp_ch_used
        };
        if ch_used > 0 {
            n += 1; // count the room for the command line
        }
        total = total.max(n);
        tp = t.tp_next;
    }
    // SAFETY: forwarded from this function's own safety doc.
    total += unsafe { tabline_height() } + unsafe { global_stl_height() };
    total
}

/// Check `'winminheight'` for a valid value and reduce it if needed
/// (`did_set_winminheight`).
///
/// `args` is accepted for signature fidelity only (the original marks
/// its own `optset_T *args` parameter `FUNC_ATTR_UNUSED` - neither
/// body ever reads it) - its type/mutability match
/// [`crate::option_defs::OptDidSetCbT`]'s own `&mut OptsetT`
/// parameter, and the return type matches its `Option<&'static [u8]>`
/// too, for eventual real wiring into a `VimoptionT.opt_did_set_cb`
/// entry. This function itself is `unsafe fn` (not a plain,
/// `OptDidSetCbT`-compatible safe `fn`), since its body genuinely
/// dereferences `GLOBALS`' raw window/frame pointers via
/// `min_rows_for_all_tabpages` - a real future dispatch wiring would
/// need a small safe shim bridging the two, matching how any other
/// genuinely-unsafe operation gets bridged to a safe call site in a
/// real running editor.
///
/// Always returns `None` (`NULL`/"no error" in the original) - the
/// real `emsg(_(e_noroom))` display is skipped (message.c's pipeline
/// not tractable), while the identical `OPTION_VARS.p_wmh` reduction
/// is kept exactly.
///
/// # Safety
/// Same as [`min_rows_for_all_tabpages`]. Touches
/// `crate::option_vars::OPTION_VARS`.
pub unsafe fn did_set_winminheight(
    _args: &mut crate::option_defs::OptsetT,
) -> Option<&'static [u8]> {
    loop {
        // SAFETY: forwarded from this function's own safety doc.
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        if opts.p_wmh <= 0 {
            break;
        }
        // SAFETY: forwarded from this function's own safety doc.
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let room = globals.Rows - opts.p_ch as i32;
        // SAFETY: forwarded from this function's own safety doc.
        let needed = unsafe { min_rows_for_all_tabpages() };
        if room >= needed {
            break;
        }
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_wmh -= 1;
    }
    None
}

/// Check `'winminwidth'` for a valid value and reduce it if needed
/// (`did_set_winminwidth`).
///
/// # Safety
/// Same as [`did_set_winminheight`]. Touches
/// `crate::option_vars::OPTION_VARS`/`crate::globals::GLOBALS.topframe`.
pub unsafe fn did_set_winminwidth(
    _args: &mut crate::option_defs::OptsetT,
) -> Option<&'static [u8]> {
    loop {
        // SAFETY: forwarded from this function's own safety doc.
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        if opts.p_wmw <= 0 {
            break;
        }
        // SAFETY: forwarded from this function's own safety doc.
        let room = unsafe { crate::globals::GLOBALS.get_mut() }.Columns;
        // SAFETY: forwarded from this function's own safety doc.
        let topframe = unsafe { crate::globals::GLOBALS.get_mut() }.topframe;
        // SAFETY: forwarded from this function's own safety doc.
        let needed = unsafe { frame_minwidth(topframe, std::ptr::null_mut()) };
        if room >= needed {
            break;
        }
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_wmw -= 1;
    }
    None
}

/// Add a status line to windows at the bottom of `frp`
/// (`frame_add_statusline`). Does NOT check if there is room, matching
/// the original's own documented caveat.
///
/// # Safety
/// `frp` must be a valid, non-null pointer to a live `FrameT`, whose
/// own `fr_child`/`fr_next` chain (if any) consists entirely of valid,
/// live `FrameT` pointers, and whose `fr_win` (for a leaf) is a valid,
/// live `WinT` pointer.
pub unsafe fn frame_add_statusline(frp: *mut crate::buffer_defs::FrameT) {
    // SAFETY: forwarded from this function's own safety doc.
    let fr = unsafe { &*frp };
    if fr.fr_layout == crate::buffer_defs::FR_LEAF {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &mut *fr.fr_win }.w_status_height = STATUS_HEIGHT;
    } else if fr.fr_layout == crate::buffer_defs::FR_ROW {
        // Handle all the frames in the row.
        let mut child = fr.fr_child;
        while !child.is_null() {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { frame_add_statusline(child) };
            // SAFETY: forwarded from this function's own safety doc.
            child = unsafe { &*child }.fr_next;
        }
    } else {
        debug_assert_eq!(fr.fr_layout, crate::buffer_defs::FR_COL);
        // Only need to handle the last frame in the column.
        let mut child = fr.fr_child;
        // SAFETY: forwarded from this function's own safety doc.
        while !unsafe { &*child }.fr_next.is_null() {
            // SAFETY: forwarded from this function's own safety doc.
            child = unsafe { &*child }.fr_next;
        }
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { frame_add_statusline(child) };
    }
}

/// Add a horizontal separator to windows at the bottom of `frp`
/// (`frame_add_hsep`).
///
/// # Safety
/// Same as [`frame_add_statusline`].
pub unsafe fn frame_add_hsep(frp: *const crate::buffer_defs::FrameT) {
    // SAFETY: forwarded from this function's own safety doc.
    let fr = unsafe { &*frp };
    if fr.fr_layout == crate::buffer_defs::FR_LEAF {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &mut *fr.fr_win }.w_hsep_height = 1;
    } else if fr.fr_layout == crate::buffer_defs::FR_ROW {
        // Handle all the frames in the row.
        let mut child = fr.fr_child;
        while !child.is_null() {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { frame_add_hsep(child) };
            // SAFETY: forwarded from this function's own safety doc.
            child = unsafe { &*child }.fr_next;
        }
    } else {
        debug_assert_eq!(fr.fr_layout, crate::buffer_defs::FR_COL);
        // Only need to handle the last frame in the column.
        let mut child = fr.fr_child;
        // SAFETY: forwarded from this function's own safety doc.
        while !unsafe { &*child }.fr_next.is_null() {
            // SAFETY: forwarded from this function's own safety doc.
            child = unsafe { &*child }.fr_next;
        }
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { frame_add_hsep(child) };
    }
}

/// Get the window `count` positions above (`up == true`) or below
/// `wp` in `tp`'s frame tree (`win_vert_neighbor`). Returns `wp`
/// itself (via `foundfr` staying `wp.w_frame`) if no such neighbor
/// exists.
///
/// # Safety
/// `wp`/`tp` must be valid, non-null pointers; `wp.w_frame`'s own
/// `fr_parent`/`fr_next`/`fr_prev`/`fr_child` chains, and `tp`'s own
/// `tp_topframe`, must consist of valid, live pointers.
#[must_use]
pub unsafe fn win_vert_neighbor(
    tp: *const crate::buffer_defs::TabpageT,
    wp: *mut WinT,
    up: bool,
    count: i32,
) -> *mut WinT {
    // SAFETY: forwarded from this function's own safety doc.
    let w = unsafe { &*wp };
    let mut foundfr = w.w_frame;

    if w.w_floating {
        // SAFETY: forwarded from this function's own safety doc.
        let prevwin = unsafe { crate::globals::GLOBALS.get_mut() }.prevwin;
        // SAFETY: forwarded from this function's own safety doc.
        return if unsafe { win_valid(prevwin) } && !unsafe { &*prevwin }.w_floating {
            prevwin
        } else {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::globals::GLOBALS.get_mut() }.firstwin
        };
    }

    let mut remaining = count;
    'outer: while remaining > 0 {
        remaining -= 1;
        // SAFETY: forwarded from this function's own safety doc.
        let mut fr = foundfr;
        let nfr;
        loop {
            if std::ptr::eq(fr, unsafe { &*tp }.tp_topframe) {
                break 'outer;
            }
            // SAFETY: forwarded from this function's own safety doc.
            let f = unsafe { &*fr };
            let candidate = if up { f.fr_prev } else { f.fr_next };
            // SAFETY: forwarded from this function's own safety doc.
            if unsafe { &*f.fr_parent }.fr_layout == crate::buffer_defs::FR_COL && !candidate.is_null() {
                nfr = candidate;
                break;
            }
            fr = f.fr_parent;
        }

        // Now go downwards to find the bottom or top frame in it.
        let mut nfr = nfr;
        loop {
            // SAFETY: forwarded from this function's own safety doc.
            let n = unsafe { &*nfr };
            if n.fr_layout == crate::buffer_defs::FR_LEAF {
                foundfr = nfr;
                break;
            }
            let mut fr = n.fr_child;
            if n.fr_layout == crate::buffer_defs::FR_ROW {
                // Find the frame at the cursor column.
                loop {
                    // SAFETY: forwarded from this function's own safety doc.
                    let f = unsafe { &*fr };
                    if f.fr_next.is_null() {
                        break;
                    }
                    // SAFETY: forwarded from this function's own safety doc.
                    let fw = unsafe { &*frame2win(fr) };
                    if fw.w_wincol + f.fr_width > w.w_wincol + w.w_wcol {
                        break;
                    }
                    fr = f.fr_next;
                }
            }
            if n.fr_layout == crate::buffer_defs::FR_COL && up {
                loop {
                    // SAFETY: forwarded from this function's own safety doc.
                    let f = unsafe { &*fr };
                    if f.fr_next.is_null() {
                        break;
                    }
                    fr = f.fr_next;
                }
            }
            nfr = fr;
        }
    }

    if foundfr.is_null() { std::ptr::null_mut() } else { unsafe { &*foundfr }.fr_win }
}

/// Get the window `count` positions to the left (`left == true`) or
/// right of `wp` in `tp`'s frame tree (`win_horz_neighbor`). Returns
/// `wp` itself (via `foundfr` staying `wp.w_frame`) if no such
/// neighbor exists.
///
/// # Safety
/// Same requirements as [`win_vert_neighbor`].
#[must_use]
pub unsafe fn win_horz_neighbor(
    tp: *const crate::buffer_defs::TabpageT,
    wp: *mut WinT,
    left: bool,
    count: i32,
) -> *mut WinT {
    // SAFETY: forwarded from this function's own safety doc.
    let w = unsafe { &*wp };
    let mut foundfr = w.w_frame;

    if w.w_floating {
        // SAFETY: forwarded from this function's own safety doc.
        let prevwin = unsafe { crate::globals::GLOBALS.get_mut() }.prevwin;
        // SAFETY: forwarded from this function's own safety doc.
        return if unsafe { win_valid(prevwin) } && !unsafe { &*prevwin }.w_floating {
            prevwin
        } else {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::globals::GLOBALS.get_mut() }.firstwin
        };
    }

    let mut remaining = count;
    'outer: while remaining > 0 {
        remaining -= 1;
        // SAFETY: forwarded from this function's own safety doc.
        let mut fr = foundfr;
        let nfr;
        loop {
            if std::ptr::eq(fr, unsafe { &*tp }.tp_topframe) {
                break 'outer;
            }
            // SAFETY: forwarded from this function's own safety doc.
            let f = unsafe { &*fr };
            let candidate = if left { f.fr_prev } else { f.fr_next };
            // SAFETY: forwarded from this function's own safety doc.
            if unsafe { &*f.fr_parent }.fr_layout == crate::buffer_defs::FR_ROW && !candidate.is_null() {
                nfr = candidate;
                break;
            }
            fr = f.fr_parent;
        }

        // Now go downwards to find the leftmost or rightmost frame in it.
        let mut nfr = nfr;
        loop {
            // SAFETY: forwarded from this function's own safety doc.
            let n = unsafe { &*nfr };
            if n.fr_layout == crate::buffer_defs::FR_LEAF {
                foundfr = nfr;
                break;
            }
            let mut fr = n.fr_child;
            if n.fr_layout == crate::buffer_defs::FR_COL {
                // Find the frame at the cursor row.
                loop {
                    // SAFETY: forwarded from this function's own safety doc.
                    let f = unsafe { &*fr };
                    if f.fr_next.is_null() {
                        break;
                    }
                    // SAFETY: forwarded from this function's own safety doc.
                    let fw = unsafe { &*frame2win(fr) };
                    if fw.w_winrow + f.fr_height > w.w_winrow + w.w_wrow {
                        break;
                    }
                    fr = f.fr_next;
                }
            }
            if n.fr_layout == crate::buffer_defs::FR_ROW && left {
                loop {
                    // SAFETY: forwarded from this function's own safety doc.
                    let f = unsafe { &*fr };
                    if f.fr_next.is_null() {
                        break;
                    }
                    fr = f.fr_next;
                }
            }
            nfr = fr;
        }
    }

    if foundfr.is_null() { std::ptr::null_mut() } else { unsafe { &*foundfr }.fr_win }
}

/// Check if `wp` is at the bottom of its column of windows - i.e.
/// there are no windows below it (`is_bottom_win`).
///
/// # Safety
/// `wp.w_frame`'s own `fr_parent` chain must consist of valid, live
/// `FrameT` pointers.
#[must_use]
pub unsafe fn is_bottom_win(wp: &WinT) -> bool {
    let mut frp = wp.w_frame;
    loop {
        // SAFETY: forwarded from this function's own safety doc.
        let fr = unsafe { &*frp };
        if fr.fr_parent.is_null() {
            return true;
        }
        // SAFETY: forwarded from this function's own safety doc.
        let parent = unsafe { &*fr.fr_parent };
        if parent.fr_layout == crate::buffer_defs::FR_COL && !fr.fr_next.is_null() {
            return false;
        }
        frp = fr.fr_parent;
    }
}

/// Set the width/height that a window will occupy, other than what's
/// used for the 'winbar'/status/vertical-separator lines
/// (`win_set_inner_size`).
///
/// Only the "no real size change" fast path (both branches' own guard
/// conditions - `height != prev_height`/`width != wp.w_view_width` -
/// false) is translated: the "real work" bodies each need substantial
/// additional machinery not yet translated (`validate_cursor`/
/// `set_fraction`/`win_comp_scroll`/`scroll_to_fraction` for the
/// height branch; `curs_columns` for the width branch - beyond the
/// pure `redraw_later` scheduling omitted per this crate's established
/// policy) - `unimplemented!()`s if either is actually reached. This
/// crate's own real caller, `winrestview()` (via [`win_new_height`]/
/// [`win_new_width`]), always calls with the window's OWN current
/// height/width, which - for any window whose `w_view_height`/
/// `w_view_width` are already consistent with its `w_height`/`w_width`
/// (the normal, already-configured case) - never triggers either
/// branch.
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT`.
pub unsafe fn win_set_inner_size(wp: *mut WinT, _valid_cursor: bool) {
    // SAFETY: forwarded from this function's own safety doc.
    let w = unsafe { &mut *wp };
    let width = if w.w_width_request == 0 { w.w_width } else { w.w_width_request };

    let prev_height = w.w_view_height;
    let height = if w.w_height_request == 0 { (w.w_height - w.w_winbar_height).max(0) } else { w.w_height_request };

    if height != prev_height {
        unimplemented!(
            "win_set_inner_size: a real height change needs validate_cursor/set_fraction/\
             win_comp_scroll/scroll_to_fraction, not yet translated"
        );
    }

    if width != w.w_view_width {
        unimplemented!(
            "win_set_inner_size: a real width change needs curs_columns, not yet translated"
        );
    }
}

/// Set the width of window `wp` (`win_new_width`).
///
/// # Safety
/// Forwarded from [`win_set_inner_size`]'s own safety doc.
pub unsafe fn win_new_width(wp: *mut WinT, width: i32) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { &mut *wp }.w_width = width.max(0);
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { &mut *wp }.w_pos_changed = true;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { win_set_inner_size(wp, true) };
}

/// Set the height of window `wp` (`win_new_height`).
///
/// # Safety
/// Forwarded from [`win_set_inner_size`]'s own safety doc.
pub unsafe fn win_new_height(wp: *mut WinT, height: i32) {
    let height = height.max(0);
    // SAFETY: forwarded from this function's own safety doc.
    let w = unsafe { &mut *wp };
    if w.w_height == height {
        // nothing to do
        return;
    }
    w.w_height = height;
    w.w_pos_changed = true;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { win_set_inner_size(wp, true) };
}

/// `split_disallowed` - depth counter; `> 0` means splitting a window
/// is currently disallowed (`window.c`'s own file-static `int
/// split_disallowed`). Mutated only by [`window_layout_lock`]/
/// [`window_layout_unlock`].
static SPLIT_DISALLOWED: GlobalCell<i32> = GlobalCell::new(0);

/// `close_disallowed` - depth counter; `> 0` means closing a window is
/// currently disallowed. Mutated only by [`window_layout_lock`]/
/// [`window_layout_unlock`].
static CLOSE_DISALLOWED: GlobalCell<i32> = GlobalCell::new(0);

/// `frame_locked` - depth counter; `> 0` means the frame layout must
/// not be changed. In the original, only ever incremented/decremented
/// by `winframe_remove`, not yet translated - so this stays `0`
/// forever in this crate today, matching the real state of any
/// session that can't yet remove a window's frame from the layout
/// tree.
static FRAME_LOCKED: GlobalCell<i32> = GlobalCell::new(0);

/// Disallow changing the window layout (splitting or closing a
/// window) until a matching [`window_layout_unlock`] call
/// (`window_layout_lock`).
pub fn window_layout_lock() {
    // SAFETY: a plain increment through one exclusive borrow at a
    // time, no aliasing hazard.
    unsafe {
        *SPLIT_DISALLOWED.get_mut() += 1;
        *CLOSE_DISALLOWED.get_mut() += 1;
    }
}

/// Undo one [`window_layout_lock`] call (`window_layout_unlock`).
pub fn window_layout_unlock() {
    // SAFETY: same as `window_layout_lock`.
    unsafe {
        *SPLIT_DISALLOWED.get_mut() -= 1;
        *CLOSE_DISALLOWED.get_mut() -= 1;
    }
}

/// `true` when the frame layout is currently locked and must not be
/// changed (`frames_locked`).
#[must_use]
pub fn frames_locked() -> bool {
    // SAFETY: a plain read through one exclusive borrow.
    unsafe { *FRAME_LOCKED.get_mut() != 0 }
}

/// `true` when splitting a window containing `wp` is currently
/// ALLOWED (`check_split_disallowed_err`'s own real sense: despite its
/// name, this returns `true` for "no problem, go ahead" and `false`
/// for "disallowed" - matching the original exactly). Omits the
/// original's own `Error *err` out-parameter entirely: it exists only
/// to carry message TEXT for a caller that displays it, and this
/// crate's established "skip the deferred message-display side
/// effect, keep the exact same return value" policy already applies
/// (matching `check_can_set_curbuf_disabled`'s own precedent).
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT` whose own
/// `w_buffer` is a valid, non-null pointer to a live `BufT`.
#[must_use]
pub unsafe fn check_split_disallowed_err(wp: *const WinT) -> bool {
    // SAFETY: a plain read through one exclusive borrow.
    if unsafe { *SPLIT_DISALLOWED.get_mut() } > 0 {
        return false;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let buf = unsafe { &*(&*wp).w_buffer };
    if buf.b_locked_split != 0 {
        return false;
    }
    true
}

/// If splitting a window containing `wp` is currently disallowed,
/// return [`crate::vim_defs::FAIL`]; otherwise return
/// [`crate::vim_defs::OK`] (`check_split_disallowed`). Omits the
/// original's real `emsg` display, matching
/// [`check_split_disallowed_err`].
///
/// # Safety
/// Same as [`check_split_disallowed_err`].
#[must_use]
pub unsafe fn check_split_disallowed(wp: *const WinT) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { check_split_disallowed_err(wp) } {
        crate::vim_defs::OK
    } else {
        crate::vim_defs::FAIL
    }
}

/// `true` when the window layout is currently LOCKED (cannot be
/// changed) - the opposite return-value sense from
/// [`check_split_disallowed_err`], matching the original's own real,
/// deliberate distinction between the two "_err" variants
/// (`window_layout_locked_err`). `cmd` only ever affects which of 2
/// possible messages would be shown (never the return value itself) -
/// kept, unused, purely for signature fidelity with the original.
#[must_use]
#[allow(unused_variables)]
pub fn window_layout_locked_err(cmd: crate::ex_cmds_defs::CmdIdxT) -> bool {
    // SAFETY: a plain read through one exclusive borrow.
    unsafe { *SPLIT_DISALLOWED.get_mut() > 0 || *CLOSE_DISALLOWED.get_mut() > 0 }
}

/// Like [`window_layout_locked_err`], but matches the original's own
/// outer wrapper name and behavior exactly (`window_layout_locked`) -
/// identical return value, since the original's own `emsg` display
/// (the only thing the wrapper adds) is already skipped.
#[must_use]
pub fn window_layout_locked(cmd: crate::ex_cmds_defs::CmdIdxT) -> bool {
    window_layout_locked_err(cmd)
}

/// Save the current window-list state into tabpage `tp`, in
/// preparation for switching away from it (`unuse_tabpage`).
///
/// # Safety
/// `tp` must be a valid, non-null pointer to a live `TabpageT`.
/// Touches `crate::globals::GLOBALS`/`crate::option_vars::OPTION_VARS`.
pub unsafe fn unuse_tabpage(tp: *mut crate::buffer_defs::TabpageT) {
    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    // SAFETY: forwarded from this function's own safety doc.
    let t = unsafe { &mut *tp };
    t.tp_topframe = g.topframe;
    t.tp_firstwin = g.firstwin;
    t.tp_lastwin = g.lastwin;
    t.tp_curwin = g.curwin;
    // SAFETY: forwarded from this function's own safety doc.
    t.tp_ch_used = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ch;
}

/// Restore the window-list state previously saved into tabpage `tp`
/// (`use_tabpage`), making it the current tabpage.
///
/// # Safety
/// Same as [`unuse_tabpage`].
pub unsafe fn use_tabpage(tp: *mut crate::buffer_defs::TabpageT) {
    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    g.curtab = tp;
    // SAFETY: forwarded from this function's own safety doc.
    let t = unsafe { &*g.curtab };
    g.topframe = t.tp_topframe;
    g.firstwin = t.tp_firstwin;
    g.lastwin = t.tp_lastwin;
    g.curwin = t.tp_curwin;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ch = t.tp_ch_used;
}

/// Check if `win` is the only non-floating window in tabpage `tp`, or
/// `None` for the current tabpage (`one_window`). Should be used in
/// place of `ONE_WINDOW` when necessary, with `firstwin` or the
/// affected window as argument depending on the situation.
///
/// The original's own `assert()` (a debug-only invariant check, not
/// functional logic) is preserved as a `debug_assert!`.
///
/// # Safety
/// `win` must be a valid, non-null pointer to a live `WinT`. `tp`, if
/// non-null, must be a valid pointer to a live `TabpageT` whose own
/// `tp_firstwin` is a valid, non-null pointer to a live `WinT`.
/// `crate::globals::GLOBALS.firstwin` must likewise be valid and
/// non-null when `tp` is null.
#[must_use]
pub unsafe fn one_window(win: *const WinT, tp: *const crate::buffer_defs::TabpageT) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    let first = if !tp.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &*tp }.tp_firstwin
    } else {
        g.firstwin
    };
    debug_assert!(
        (tp.is_null() || !std::ptr::eq(tp, g.curtab))
            // SAFETY: forwarded from this function's own safety doc.
            && !unsafe { &*first }.w_floating
    );
    // SAFETY: forwarded from this function's own safety doc.
    let w = unsafe { &*win };
    std::ptr::eq(first, win) && (w.w_next.is_null() || unsafe { &*w.w_next }.w_floating)
}

/// Check if `win` is the only window that exists, across every
/// tabpage (`last_window`).
///
/// # Safety
/// Forwarded from [`one_window`]'s own safety doc (called here with a
/// null `tp`), plus `crate::globals::GLOBALS.first_tabpage`'s own
/// `tp_next` chain must consist of valid, live `TabpageT` pointers.
#[must_use]
pub unsafe fn last_window(win: *const WinT) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    if !unsafe { one_window(win, std::ptr::null()) } {
        return false;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let first_tabpage = unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { &*first_tabpage }.tp_next.is_null()
}

/// Check if floating windows in tabpage `tp` can be closed
/// (`can_close_floating_windows`). `tp` must be null for the current
/// tabpage. Must not be called when the ctx window is in use.
///
/// The original's own `assert()` (a debug-only invariant check, not
/// functional logic) is preserved as a `debug_assert!`.
///
/// # Safety
/// `tp`, if non-null, must be a valid pointer to a live `TabpageT`
/// whose own `tp_lastwin` is a valid, non-null pointer to a live
/// `WinT`. `crate::globals::GLOBALS.lastwin` must likewise be valid
/// and non-null when `tp` is null. Whichever chain is walked (via
/// `w_prev`, for as long as `w_floating` is true) must consist
/// entirely of valid, live `WinT` pointers, each with a valid,
/// non-null `w_buffer`.
#[must_use]
pub unsafe fn can_close_floating_windows(tp: *const crate::buffer_defs::TabpageT) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    debug_assert!(
        !std::ptr::eq(tp, g.curtab.cast_const())
            && (!tp.is_null() || !crate::context::is_ctx_win(g.lastwin))
    );
    let mut wp = if !tp.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &*tp }.tp_lastwin
    } else {
        g.lastwin
    };
    // SAFETY: forwarded from this function's own safety doc.
    while unsafe { &*wp }.w_floating {
        // SAFETY: forwarded from this function's own safety doc.
        let w = unsafe { &*wp };
        // SAFETY: forwarded from this function's own safety doc.
        let buf = unsafe { &mut *w.w_buffer };
        // SAFETY: forwarded from this function's own safety doc.
        let need_hide = unsafe { crate::undo::buf_is_changed(buf) } && buf.b_nwindows <= 1;
        // SAFETY: forwarded from this function's own safety doc.
        if need_hide && !unsafe { crate::buffer::buf_hide(buf) } {
            return false;
        }
        wp = w.w_prev;
    }
    true
}

/// Called when leaving `win` (switching away from it, entering
/// another tabpage, or `ctx_switch()`) - pairs with
/// [`entering_window`]. Only matters for a prompt buffer
/// (`leaving_window`).
///
/// # Safety
/// `win` must be a valid, non-null pointer to a live `WinT` whose own
/// `w_buffer` is a valid, non-null pointer to a live `BufT`. Touches
/// `crate::globals::GLOBALS`.
pub unsafe fn leaving_window(win: *mut WinT) {
    // SAFETY: forwarded from this function's own safety doc.
    let w = unsafe { &mut *win };
    // SAFETY: forwarded from this function's own safety doc.
    let buf = unsafe { &mut *w.w_buffer };
    // Not for a ctx window: it shows its buffer only temporarily.
    if !crate::buffer::bt_prompt(Some(buf)) || crate::context::is_ctx_win(win) {
        return;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };

    // When leaving a prompt window stop Insert mode and perhaps
    // restart it when entering that window again.
    buf.b_prompt_insert = g.restart_edit;
    if g.restart_edit != 0 && g.mode_displayed {
        g.clear_cmdline = true; // unshow mode later
    }
    g.restart_edit = 0;

    // When leaving the window (or closing the window) was done from a
    // callback we need to break out of the Insert mode loop and
    // restart Insert mode when entering the window again.
    if (g.State & crate::state_defs::mode::INSERT as i32) != 0 && !g.Ins.stop_insert_mode {
        g.Ins.stop_insert_mode = true;
        if buf.b_prompt_insert == 0 {
            buf.b_prompt_insert = i32::from(b'A');
        }
    }
}

/// Called when `win` becomes `curwin`: switching to it, entering its
/// tabpage, or `ctx_restore()`. Pairs with [`leaving_window`]. Only
/// matters for a prompt buffer (`entering_window`).
///
/// # Safety
/// Same as [`leaving_window`].
pub unsafe fn entering_window(win: *mut WinT) {
    // SAFETY: forwarded from this function's own safety doc.
    let w = unsafe { &mut *win };
    // SAFETY: forwarded from this function's own safety doc.
    let buf = unsafe { &mut *w.w_buffer };
    // Not for a ctx window: it shows its buffer only temporarily.
    if !crate::buffer::bt_prompt(Some(buf)) || crate::context::is_ctx_win(win) {
        return;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };

    // When switching to a prompt buffer that was in Insert mode,
    // don't stop Insert mode, it may have been set in
    // leaving_window().
    if buf.b_prompt_insert != 0 {
        g.Ins.stop_insert_mode = false;
    }

    // When entering the prompt window restart Insert mode if we were
    // in Insert mode when we left it and not already in Insert mode.
    if (g.State & crate::state_defs::mode::INSERT as i32) == 0 {
        g.restart_edit = buf.b_prompt_insert;
    }
}

/// Trigger the `WinNewPre` autocmd, wrapped in a window-layout lock
/// (`trigger_winnewpre`). Since `apply_autocmds` is real but nothing
/// in this crate can currently register a `WinNewPre` autocmd, this
/// is (correctly, faithfully) a real no-op today - not a stub.
pub fn trigger_winnewpre() {
    window_layout_lock();
    let _ =
        crate::autocmd::apply_autocmds(crate::autocmd_defs::EventT::WinNewPre, None, None, false, None);
    window_layout_unlock();
}

/// `recursive` guard for [`do_autocmd_winclosed`] (a real, function-
/// local C `static`, matching the original exactly).
static DO_AUTOCMD_WINCLOSED_RECURSIVE: GlobalCell<bool> = GlobalCell::new(false);

/// Trigger the `WinClosed` autocmd for `win` (`do_autocmd_winclosed`).
/// The real autocmd-triggering body (formatting a winid string via
/// `vim_snprintf` and calling `apply_autocmds`) is `unimplemented!()` -
/// unreachable today, since nothing in this crate can currently
/// register a real `WinClosed` autocmd (`has_event` is always false).
#[allow(dead_code)]
pub fn do_autocmd_winclosed(_win: *const WinT) {
    // SAFETY: a plain read through one exclusive borrow.
    let recursive = unsafe { *DO_AUTOCMD_WINCLOSED_RECURSIVE.get_mut() };
    if recursive || !crate::autocmd::has_event(crate::autocmd_defs::EventT::WinClosed) {
        return;
    }
    unimplemented!(
        "do_autocmd_winclosed: real WinClosed autocmd execution needs vim_snprintf, \
         unreachable today since has_event(WinClosed) is always false"
    );
}

/// `recursive` guard for [`trigger_tabclosedpre`] (a real, function-
/// local C `static`, matching the original exactly).
static TRIGGER_TABCLOSEDPRE_RECURSIVE: GlobalCell<bool> = GlobalCell::new(false);

/// Trigger the `TabClosedPre` autocmd for tabpage `tp`
/// (`trigger_tabclosedpre`). The real autocmd-triggering body (a
/// tabpage switch via `goto_tabpage_tp`, not yet translated) is
/// `unimplemented!()` - unreachable today, since nothing in this
/// crate can currently register a real `TabClosedPre` autocmd
/// (`has_event` is always false).
#[allow(dead_code)]
pub fn trigger_tabclosedpre(_tp: *const crate::buffer_defs::TabpageT) {
    // SAFETY: a plain read through one exclusive borrow.
    let recursive = unsafe { *TRIGGER_TABCLOSEDPRE_RECURSIVE.get_mut() };
    if !crate::autocmd::has_event(crate::autocmd_defs::EventT::TabClosedPre) || recursive {
        return;
    }
    unimplemented!(
        "trigger_tabclosedpre: real TabClosedPre autocmd execution needs goto_tabpage_tp, \
         unreachable today since has_event(TabClosedPre) is always false"
    );
}

/// Return the number of lines used by the global statusline/winbar,
/// i.e. whether `'winbar'` is set globally (`global_winbar_height`).
///
/// # Safety
/// Touches `crate::option_vars::OPTION_VARS`.
#[must_use]
pub unsafe fn global_winbar_height() -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let p_wbr = &unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_wbr;
    i32::from(p_wbr.as_deref().is_some_and(|s| !s.is_empty()))
}

/// Compute the maximum number of windows that fit within `height` in
/// frame `fr` (`get_maximum_wincount`).
///
/// # Safety
/// `fr` must be a valid, non-null pointer to a live `FrameT` whose own
/// `fr_child`/`fr_next` chain (if any) consists of valid, live
/// `FrameT` pointers, each with a `fr_win` (directly, or via nested
/// leaves reachable through `frame2win`) that is a valid, non-null
/// `WinT` pointer. Touches `crate::option_vars::OPTION_VARS`.
#[must_use]
pub unsafe fn get_maximum_wincount(fr: *const crate::buffer_defs::FrameT, height: i32) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let frame = unsafe { &*fr };
    let p_wmh = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_wmh as i32;

    if frame.fr_layout != crate::buffer_defs::FR_COL {
        // SAFETY: forwarded from this function's own safety doc.
        let wp = unsafe { &*frame2win(fr) };
        return height / (p_wmh + STATUS_HEIGHT + wp.w_winbar_height);
    }
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { global_winbar_height() } != 0 {
        // If winbar is globally enabled, no need to check each window
        // for it.
        return height / (p_wmh + STATUS_HEIGHT + 1);
    }

    let mut height = height;
    let mut total_wincount = 0;

    // First, try to fit all child frames of `fr` into `height`.
    let mut frp = frame.fr_child;
    while !frp.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        let wp = unsafe { &*frame2win(frp) };
        if height < p_wmh + STATUS_HEIGHT + wp.w_winbar_height {
            break;
        }
        height -= p_wmh + STATUS_HEIGHT + wp.w_winbar_height;
        total_wincount += 1;
        // SAFETY: forwarded from this function's own safety doc.
        frp = unsafe { &*frp }.fr_next;
    }

    // If we still have enough room for more windows, just use the
    // default winbar height (which is 0) in order to get the amount
    // of windows that'd fit in the remaining space.
    total_wincount += height / (p_wmh + STATUS_HEIGHT);

    total_wincount
}

/// `true` if the current tabpage has only one "real" window
/// (`only_one_window`) - used for `:only`/`:qa`-style checks. A
/// window counts unless it's a help/floating/preview window (and
/// isn't the current window), or the ctx window. If there is another
/// tabpage, there's always another window.
///
/// # Safety
/// `crate::globals::GLOBALS.first_tabpage`'s own `tp_next` chain must
/// consist of valid, live `TabpageT` pointers.
/// `crate::globals::GLOBALS.firstwin`'s own `w_next` chain must
/// consist of valid, live `WinT` pointers, each with a `w_buffer`
/// (when non-null) that is a valid, live `BufT` pointer.
/// `crate::globals::GLOBALS.curbuf`, if non-null, must likewise be
/// valid.
#[must_use]
pub unsafe fn only_one_window() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    // If there is another tab page there always is another window.
    // SAFETY: forwarded from this function's own safety doc.
    if !unsafe { &*g.first_tabpage }.tp_next.is_null() {
        return false;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = if g.curbuf.is_null() { None } else { Some(unsafe { &*g.curbuf }) };
    let mut count = 0;
    let mut wp = g.firstwin;
    while !wp.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        let w = unsafe { &*wp };
        // SAFETY: forwarded from this function's own safety doc.
        let buf = if w.w_buffer.is_null() { None } else { Some(unsafe { &*w.w_buffer }) };
        if buf.is_some()
            && (!((crate::buffer::bt_help(buf) && !crate::buffer::bt_help(curbuf))
                || w.w_floating
                || w.w_onebuf_opt.wo_pvw != 0)
                || std::ptr::eq(wp, g.curwin))
            && !crate::context::is_ctx_win(wp)
        {
            count += 1;
        }
        wp = w.w_next;
    }
    count <= 1
}

/// `last_win_id` - the handle assigned to the most recently allocated
/// window (`window.c`'s own file-static `int last_win_id`, starting
/// one below [`LOWEST_WIN_ID`], matching the original exactly). Only
/// ever incremented by `win_alloc`, not yet translated - so this
/// stays at its initial value forever in this crate today.
static LAST_WIN_ID: GlobalCell<i32> = GlobalCell::new(LOWEST_WIN_ID - 1);

/// The handle most recently assigned to a new window
/// (`get_last_winid`).
#[must_use]
pub fn get_last_winid() -> i32 {
    // SAFETY: a plain read through one exclusive borrow.
    unsafe { *LAST_WIN_ID.get_mut() }
}

/// Don't let autocommands close the given window (`win_locked`).
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT`.
#[must_use]
pub unsafe fn win_locked(wp: *const WinT) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { &*wp }.w_locked
}

/// Merge `src` into `dst`, replacing it entirely (`merge_win_config`).
///
/// The original's own explicit `clear_virttext(&dst->title_chunks)`/
/// `clear_virttext(&dst->footer_chunks)` calls (freeing `dst`'s OLD
/// virtual-text chunk data before the assignment overwrites the
/// pointer, to avoid a C-style memory leak) have NO Rust equivalent
/// here: this crate's own `WinConfig.title_chunks`/`footer_chunks`
/// are real, owned `Vec<VirtTextChunk>`s (via `VirtTextChunk.text:
/// NvimString = Vec<u8>`, not a raw pointer), so the plain assignment
/// below already drops `dst`'s previous field values automatically,
/// including any of their own owned heap data - matching this
/// crate's established "Rust's own ownership model already does what
/// the C `xfree`/`clear_virttext` dance does manually" pattern (e.g.
/// `optval_free`/`tv_dict_free_contents`).
pub fn merge_win_config(dst: &mut crate::buffer_defs::WinConfig, src: crate::buffer_defs::WinConfig) {
    *dst = src;
}

/// Clear fields in `fconfig` that are only used for floating windows.
/// Also clears fields unused after configure time, like width/height
/// (`clear_float_config`).
///
/// The original's own `free_fields` parameter distinguishes whether
/// [`merge_win_config`]'s manual virtual-text-clearing dance runs
/// before the reset - a distinction with NO observable effect in this
/// crate, since (per [`merge_win_config`]'s own doc comment) Rust's
/// plain struct assignment always correctly frees the old owned data
/// regardless. Both branches of the original's own `if
/// (free_fields) {...} else {...}` therefore produce the exact same
/// result here - `free_fields` is kept, unused, purely for signature
/// fidelity with the original.
#[allow(unused_variables)]
pub fn clear_float_config(fconfig: &mut crate::buffer_defs::WinConfig, free_fields: bool) {
    let saved_style = fconfig.style;
    let saved_cmdline_offset = fconfig._cmdline_offset;
    *fconfig = crate::buffer_defs::WinConfig::default();
    fconfig.style = saved_style;
    fconfig._cmdline_offset = saved_cmdline_offset;
}

/// Whether tabpage `tpc` is a valid, reachable tabpage with at least
/// one valid window (`valid_tabpage_win`).
///
/// # Safety
/// `crate::globals::GLOBALS.first_tabpage`'s own `tp_next` chain must
/// consist of valid, live `TabpageT` pointers. For each such tabpage,
/// its own window list (`tp_firstwin`/`w_next`, or
/// `GLOBALS.firstwin`/`w_next` when it's the current tabpage) must
/// consist of valid, live `WinT` pointers.
#[must_use]
pub unsafe fn valid_tabpage_win(tpc: *const crate::buffer_defs::TabpageT) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let mut tp = unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage;
    while !tp.is_null() {
        if std::ptr::eq(tp, tpc) {
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
                if unsafe { win_valid_any_tab(wp) } {
                    return true;
                }
                // SAFETY: forwarded from this function's own safety doc.
                wp = unsafe { &*wp }.w_next;
            }
            return false;
        }
        // SAFETY: forwarded from this function's own safety doc.
        tp = unsafe { &*tp }.tp_next;
    }
    // shouldn't happen
    false
}

/// Implementation of [`check_lnums`] and [`check_lnums_nested`]
/// (`check_lnums_both`).
///
/// # Safety
/// `crate::globals::GLOBALS.first_tabpage`'s own `tp_next` chain must
/// consist of valid, live `TabpageT` pointers. For each such tabpage,
/// its own window list (`tp_firstwin`/`w_next`, or
/// `GLOBALS.firstwin`/`w_next` when it's the current tabpage) must
/// consist of valid, live `WinT` pointers, and `GLOBALS.curbuf` must
/// be a valid, live `BufT` pointer.
unsafe fn check_lnums_both(do_curwin: bool, nested: bool) {
    // SAFETY: forwarded from this function's own safety doc.
    let globals = unsafe { crate::globals::GLOBALS.get_mut() };
    let curwin = globals.curwin;
    let curbuf = globals.curbuf;
    // SAFETY: forwarded from this function's own safety doc.
    let line_count = unsafe { &*curbuf }.b_ml.ml_line_count;

    let mut tp = globals.first_tabpage;
    while !tp.is_null() {
        let is_curtab = std::ptr::eq(tp, globals.curtab);
        let mut wp = if is_curtab {
            globals.firstwin
        } else {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { &*tp }.tp_firstwin
        };
        while !wp.is_null() {
            // SAFETY: forwarded from this function's own safety doc.
            let w = unsafe { &mut *wp };
            if (do_curwin || !std::ptr::eq(wp, curwin)) && std::ptr::eq(w.w_buffer, curbuf) {
                if !nested {
                    // save the original cursor position and topline
                    w.w_save_cursor.w_cursor_save = w.w_cursor;
                    w.w_save_cursor.w_topline_save = w.w_topline;
                }

                let mut need_adjust = w.w_cursor.lnum > line_count;
                if need_adjust {
                    w.w_cursor.lnum = line_count;
                }
                if need_adjust || !nested {
                    // save the (corrected) cursor position
                    w.w_save_cursor.w_cursor_corr = w.w_cursor;
                }

                need_adjust = w.w_topline > line_count;
                if need_adjust {
                    w.w_topline = line_count;
                }
                if need_adjust || !nested {
                    // save the (corrected) topline
                    w.w_save_cursor.w_topline_corr = w.w_topline;
                }
            }
            wp = w.w_next;
        }
        // SAFETY: forwarded from this function's own safety doc.
        tp = unsafe { &*tp }.tp_next;
    }
}

/// Correct the cursor line number in other windows. Used after
/// changing the current buffer, and before applying autocommands
/// (`check_lnums`).
///
/// `do_curwin`: when `true`, also check the current window.
///
/// # Safety
/// Same as `check_lnums_both`.
pub unsafe fn check_lnums(do_curwin: bool) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { check_lnums_both(do_curwin, false) };
}

/// Like [`check_lnums`] but for when `check_lnums` was already called
/// (`check_lnums_nested`).
///
/// # Safety
/// Same as `check_lnums_both`.
pub unsafe fn check_lnums_nested(do_curwin: bool) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { check_lnums_both(do_curwin, true) };
}

/// Reset cursor and topline to their stored values from
/// [`check_lnums`]. `check_lnums` must have been called first
/// (`reset_lnums`).
///
/// # Safety
/// Same as `check_lnums_both`.
pub unsafe fn reset_lnums() {
    // SAFETY: forwarded from this function's own safety doc.
    let globals = unsafe { crate::globals::GLOBALS.get_mut() };
    let curbuf = globals.curbuf;
    // SAFETY: forwarded from this function's own safety doc.
    let line_count = unsafe { &*curbuf }.b_ml.ml_line_count;

    let mut tp = globals.first_tabpage;
    while !tp.is_null() {
        let is_curtab = std::ptr::eq(tp, globals.curtab);
        let mut wp = if is_curtab {
            globals.firstwin
        } else {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { &*tp }.tp_firstwin
        };
        while !wp.is_null() {
            // SAFETY: forwarded from this function's own safety doc.
            let w = unsafe { &mut *wp };
            if std::ptr::eq(w.w_buffer, curbuf) {
                // Restore the value if the autocommand didn't change
                // it and it was set.
                //
                // Note: this triggers e.g. on BufReadPre, when the
                // buffer is not yet loaded, so cannot validate the
                // buffer line.
                if crate::mark_defs::equalpos(w.w_save_cursor.w_cursor_corr, w.w_cursor)
                    && w.w_save_cursor.w_cursor_save.lnum != 0
                {
                    w.w_cursor = w.w_save_cursor.w_cursor_save;
                }
                if w.w_save_cursor.w_topline_corr == w.w_topline
                    && w.w_save_cursor.w_topline_save != 0
                {
                    w.w_topline = w.w_save_cursor.w_topline_save;
                }
                if w.w_save_cursor.w_topline_save > line_count {
                    w.w_valid &= !i32::from(crate::buffer_defs::w_valid::VALID_TOPLINE);
                }
            }
            wp = w.w_next;
        }
        // SAFETY: forwarded from this function's own safety doc.
        tp = unsafe { &*tp }.tp_next;
    }
}

/// Return `true` if every frame in a row starting at `topfrp` has
/// exactly `height` (`frame_check_height`).
///
/// # Safety
/// `topfrp` must be a valid, non-null pointer to a live `FrameT`,
/// whose own `fr_child`/`fr_next` chain (if any) consists entirely of
/// valid, live `FrameT` pointers.
#[must_use]
pub unsafe fn frame_check_height(topfrp: *const crate::buffer_defs::FrameT, height: i32) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let topfr = unsafe { &*topfrp };
    if topfr.fr_height != height {
        return false;
    }
    if topfr.fr_layout == crate::buffer_defs::FR_ROW {
        let mut frp = topfr.fr_child;
        while !frp.is_null() {
            // SAFETY: forwarded from this function's own safety doc.
            let fr = unsafe { &*frp };
            if fr.fr_height != height {
                return false;
            }
            frp = fr.fr_next;
        }
    }
    true
}

/// Return `true` if every frame in a column starting at `topfrp` has
/// exactly `width` (`frame_check_width`).
///
/// # Safety
/// Same as [`frame_check_height`].
#[must_use]
pub unsafe fn frame_check_width(topfrp: *const crate::buffer_defs::FrameT, width: i32) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let topfr = unsafe { &*topfrp };
    if topfr.fr_width != width {
        return false;
    }
    if topfr.fr_layout == crate::buffer_defs::FR_COL {
        let mut frp = topfr.fr_child;
        while !frp.is_null() {
            // SAFETY: forwarded from this function's own safety doc.
            let fr = unsafe { &*frp };
            if fr.fr_width != width {
                return false;
            }
            frp = fr.fr_next;
        }
    }
    true
}

/// Check `cc` as `'colorcolumn'` and update the members of `wp`. This
/// is called when `'colorcolumn'` or `'textwidth'` is changed
/// (`check_colorcolumn`).
///
/// `cc`: when `None`, use `wp`'s own `'colorcolumn'` value.
///
/// `wp`: when `None`, only parse `cc` (don't update anything).
///
/// Returns `false` on a malformed `'colorcolumn'` value, matching the
/// original's own real, non-null-vs-null error-message return (`true`
/// = `NULL` = valid) - the message text itself (`e_invarg`) is
/// omitted, matching this crate's established "skip the deferred
/// message-display side effect, keep the exact same return value"
/// policy, and matching `valid_name`/`check_ff_value`'s own precedent
/// of a plain `bool` for this exact "valid or not" shape. The
/// original's own debug-only overflow-safety `assert()` before
/// `col += (int)tw` has no Rust equivalent needed: `wrapping_add`
/// below makes the addition itself well-defined (never UB) regardless,
/// unlike the original's plain `+=`.
///
/// # Safety
/// If `wp` is `Some`, its own `w_buffer` must either be null (meaning
/// "buffer was closed") or a valid, live `BufT` pointer.
#[must_use]
pub unsafe fn check_colorcolumn(cc: Option<&[u8]>, wp: Option<&mut WinT>) -> bool {
    if let Some(ref w) = wp
        && w.w_buffer.is_null()
    {
        return true; // buffer was closed
    }

    // Resolve the effective string to parse: cc if given, else wp's
    // own w_p_cc, else empty.
    let owned_cc;
    let s: &[u8] = if let Some(cc) = cc {
        cc
    } else if let Some(ref w) = wp {
        owned_cc = w.w_onebuf_opt.wo_cc.clone().unwrap_or_default();
        &owned_cc
    } else {
        &[]
    };

    let tw: crate::types_defs::OptInt = if let Some(ref w) = wp {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &*w.w_buffer }.b_p_tw
    } else {
        0 // buffer-local value not set, assume zero
    };

    let mut color_cols: Vec<i32> = Vec::new();
    let mut pos = 0;
    while pos < s.len() && color_cols.len() < 255 {
        // Whether this item should actually be added to color_cols -
        // false mirrors the original's own `goto skip;` (the item is
        // silently dropped, but parsing continues).
        let mut pushed = true;
        let col = if s[pos] == b'-' || s[pos] == b'+' {
            // -N and +N: add to 'textwidth'.
            let sign: i32 = if s[pos] == b'-' { -1 } else { 1 };
            pos += 1;
            if !(pos < s.len() && crate::ascii_defs::ascii_isdigit(i32::from(s[pos]))) {
                return false;
            }
            let (digits, consumed) = crate::charset::getdigits_int(&s[pos..], true, 0);
            pos += consumed;
            let col = sign * digits;
            if tw == 0 {
                // 'textwidth' not set, skip this item.
                pushed = false;
                0
            } else {
                let col = col.wrapping_add(tw as i32);
                if col < 0 {
                    pushed = false;
                }
                col
            }
        } else if crate::ascii_defs::ascii_isdigit(i32::from(s[pos])) {
            let (digits, consumed) = crate::charset::getdigits_int(&s[pos..], true, 0);
            pos += consumed;
            digits
        } else {
            return false;
        };
        if pushed {
            color_cols.push(col - 1); // 1-based to 0-based
        }

        if pos >= s.len() {
            break;
        }
        if s[pos] != b',' {
            return false;
        }
        pos += 1;
        if pos >= s.len() {
            return false; // illegal trailing comma as in "set cc=80,"
        }
    }

    let Some(w) = wp else {
        return true; // only parse cc
    };

    if color_cols.is_empty() {
        w.w_p_cc_cols = None;
    } else {
        // sort the columns for faster usage on screen redraw inside
        // win_line()
        color_cols.sort_unstable();
        let mut deduped: Vec<i32> = Vec::with_capacity(color_cols.len());
        for c in color_cols {
            // skip duplicates
            if deduped.last() != Some(&c) {
                deduped.push(c);
            }
        }
        w.w_p_cc_cols = Some(deduped);
    }

    true
}

/// Return the last non-floating window in tabpage `tp`, or in the
/// current tab page if `tp` is null (`lastwin_nofloating`).
///
/// # Safety
/// `tp` (if non-null) must be a valid, live `TabpageT` pointer, and
/// its own `tp_lastwin`, or `crate::globals::GLOBALS.lastwin` when
/// `tp` is null, must start a `w_prev` chain consisting entirely of
/// valid, live `WinT` pointers with at least one non-floating entry.
#[must_use]
pub unsafe fn lastwin_nofloating(tp: *const crate::buffer_defs::TabpageT) -> *mut WinT {
    debug_assert!(
        tp.is_null() || !std::ptr::eq(tp, unsafe { crate::globals::GLOBALS.get_mut() }.curtab),
        "lastwin_nofloating: pass null instead of curtab explicitly"
    );
    let mut res = if !tp.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &*tp }.tp_lastwin
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::globals::GLOBALS.get_mut() }.lastwin
    };
    // SAFETY: forwarded from this function's own safety doc.
    while unsafe { &*res }.w_floating {
        // SAFETY: forwarded from this function's own safety doc.
        res = unsafe { &*res }.w_prev;
    }
    res
}

/// Return the height reserved for the last window's status line, or
/// `0` if none is shown (`last_stl_height`).
///
/// `morewin`: when `true`, count as if there will always be more than
/// one window (used when about to add a new one).
///
/// # Safety
/// `crate::globals::GLOBALS.firstwin`'s own `w_next` chain (used by
/// [`one_window`]) must consist of valid, live `WinT` pointers.
#[must_use]
pub unsafe fn last_stl_height(morewin: bool) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let p_ls = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ls;
    // SAFETY: forwarded from this function's own safety doc.
    let firstwin = unsafe { crate::globals::GLOBALS.get_mut() }.firstwin;
    let show = p_ls > 1
        || (p_ls == 1
            // SAFETY: forwarded from this function's own safety doc.
            && (morewin || !unsafe { one_window(firstwin, std::ptr::null()) }));
    if show { STATUS_HEIGHT } else { 0 }
}

/// Build a dict describing a window's size/scroll-position change:
/// `{width, height, topline, topfill, leftcol, skipcol}`
/// (`make_win_info_dict`). Returns `None` if any entry could not be
/// added, matching the original's own real (if practically
/// unreachable) `tv_dict_add_tv` failure path - every key here is a
/// distinct literal, so `tv_dict_add`'s only failure mode (a
/// duplicate key already present) can never actually occur, but the
/// original's own "stop at the first failure, release the dict,
/// return NULL" structure is preserved faithfully anyway via `&&`'s
/// own left-to-right short-circuit evaluation.
///
/// The returned dict (if any) is a freshly-allocated dict the caller
/// now owns and must eventually release via `tv_dict_unref`.
#[must_use]
pub fn make_win_info_dict(
    width: i32,
    height: i32,
    topline: i32,
    topfill: i32,
    leftcol: i32,
    skipcol: i32,
) -> Option<*mut crate::eval::typval_defs::DictT> {
    let d = crate::eval::typval::tv_dict_alloc();
    // SAFETY: `d` was just allocated above, so it's a valid, exclusive,
    // not-yet-shared pointer.
    let dict = unsafe { &mut *d };
    dict.dv_refcount = 1;

    let ok = crate::eval::typval::tv_dict_add_nr(dict, b"width", i64::from(width))
        != crate::vim_defs::FAIL
        && crate::eval::typval::tv_dict_add_nr(dict, b"height", i64::from(height))
            != crate::vim_defs::FAIL
        && crate::eval::typval::tv_dict_add_nr(dict, b"topline", i64::from(topline))
            != crate::vim_defs::FAIL
        && crate::eval::typval::tv_dict_add_nr(dict, b"topfill", i64::from(topfill))
            != crate::vim_defs::FAIL
        && crate::eval::typval::tv_dict_add_nr(dict, b"leftcol", i64::from(leftcol))
            != crate::vim_defs::FAIL
        && crate::eval::typval::tv_dict_add_nr(dict, b"skipcol", i64::from(skipcol))
            != crate::vim_defs::FAIL;

    if ok {
        Some(d)
    } else {
        // SAFETY: `d` is a freshly-allocated, exclusively-owned dict.
        unsafe { crate::eval::typval::tv_dict_unref(d) };
        None
    }
}

/// Return the tabpage to switch to when closing the current tab page
/// (`alt_tabpage`).
///
/// # Safety
/// `crate::globals::GLOBALS.curtab` must be a valid, non-null pointer
/// to a live `TabpageT`. `GLOBALS.first_tabpage`'s own `tp_next` chain
/// must consist of valid, live `TabpageT` pointers and must contain
/// `curtab` somewhere in that chain.
#[must_use]
pub unsafe fn alt_tabpage() -> *mut crate::buffer_defs::TabpageT {
    // SAFETY: forwarded from this function's own safety doc.
    let globals = unsafe { crate::globals::GLOBALS.get_mut() };
    // SAFETY: forwarded from this function's own safety doc.
    let tcl_flags = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.tcl_flags;

    // Use the last accessed tab page, if possible.
    if tcl_flags & crate::option_vars::opt_tcl_flag::USELAST != 0
        // SAFETY: forwarded from this function's own safety doc.
        && unsafe { valid_tabpage(globals.lastused_tabpage) }
    {
        return globals.lastused_tabpage;
    }

    // Use the next tab page, if possible.
    // SAFETY: forwarded from this function's own safety doc.
    let curtab_next = unsafe { &*globals.curtab }.tp_next;
    let forward = !curtab_next.is_null()
        && (tcl_flags & crate::option_vars::opt_tcl_flag::LEFT == 0
            || std::ptr::eq(globals.curtab, globals.first_tabpage));

    if forward {
        curtab_next
    } else {
        // Use the previous tab page.
        let mut tp = globals.first_tabpage;
        // SAFETY: forwarded from this function's own safety doc.
        while !std::ptr::eq(unsafe { &*tp }.tp_next, globals.curtab) {
            // SAFETY: forwarded from this function's own safety doc.
            tp = unsafe { &*tp }.tp_next;
        }
        tp
    }
}

/// Insert frame `frp` in a frame list, right after `after`
/// (`frame_append`).
///
/// # Safety
/// `after` must be a valid, non-null pointer to a live `FrameT`,
/// distinct from `frp`. `frp` must be a valid, non-null pointer to a
/// live `FrameT` not already linked into any frame list. If `after`
/// has a next sibling, that sibling must also be a valid, live
/// `FrameT` pointer.
pub unsafe fn frame_append(
    after: *mut crate::buffer_defs::FrameT,
    frp: *mut crate::buffer_defs::FrameT,
) {
    // SAFETY: forwarded from this function's own safety doc.
    let after_ref = unsafe { &mut *after };
    // SAFETY: forwarded from this function's own safety doc.
    let frp_ref = unsafe { &mut *frp };
    frp_ref.fr_next = after_ref.fr_next;
    after_ref.fr_next = frp;
    if !frp_ref.fr_next.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &mut *frp_ref.fr_next }.fr_prev = frp;
    }
    frp_ref.fr_prev = after;
}

/// Insert frame `frp` in a frame list, right before `before`
/// (`frame_insert`).
///
/// # Safety
/// `before` must be a valid, non-null pointer to a live `FrameT`,
/// distinct from `frp`, whose own `fr_prev` (if non-null) is also a
/// valid, live pointer. `frp` must be a valid, non-null pointer to a
/// live `FrameT` not already linked into any frame list; if `before`
/// has no previous sibling (`frp` will become the new head), `frp`'s
/// own `fr_parent` must already be set to a valid, live `FrameT`
/// pointer by the caller before calling this function (matching the
/// original's own `frp->fr_parent->fr_child = frp;` - it reads `frp`'s
/// OWN `fr_parent` field, not `before`'s).
pub unsafe fn frame_insert(
    before: *mut crate::buffer_defs::FrameT,
    frp: *mut crate::buffer_defs::FrameT,
) {
    // SAFETY: forwarded from this function's own safety doc.
    let before_ref = unsafe { &mut *before };
    // SAFETY: forwarded from this function's own safety doc.
    let frp_ref = unsafe { &mut *frp };
    frp_ref.fr_next = before;
    frp_ref.fr_prev = before_ref.fr_prev;
    before_ref.fr_prev = frp;
    if !frp_ref.fr_prev.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &mut *frp_ref.fr_prev }.fr_next = frp;
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &mut *frp_ref.fr_parent }.fr_child = frp;
    }
}

/// Remove frame `frp` from its frame list (`frame_remove`).
///
/// # Safety
/// `frp` must be a valid, non-null pointer to a live `FrameT`
/// currently linked into a frame list, whose own `fr_prev` (if
/// non-null) or `fr_parent` (if `fr_prev` is null) is also a valid,
/// live pointer, and whose own `fr_next` (if non-null) is also a
/// valid, live pointer.
pub unsafe fn frame_remove(frp: *mut crate::buffer_defs::FrameT) {
    // SAFETY: forwarded from this function's own safety doc.
    let frp_ref = unsafe { &mut *frp };
    if !frp_ref.fr_prev.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &mut *frp_ref.fr_prev }.fr_next = frp_ref.fr_next;
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &mut *frp_ref.fr_parent }.fr_child = frp_ref.fr_next;
    }
    if !frp_ref.fr_next.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &mut *frp_ref.fr_next }.fr_prev = frp_ref.fr_prev;
    }
}

/// Whether frame `frp` (or one of its descendants) directly contains
/// window `wp` (`frame_has_win`).
///
/// # Safety
/// `frp` must be a valid, non-null pointer to a live `FrameT`, whose
/// own `fr_child`/`fr_next` chain (if any) consists entirely of valid,
/// live `FrameT` pointers.
#[must_use]
pub unsafe fn frame_has_win(frp: *const crate::buffer_defs::FrameT, wp: *const WinT) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let fr = unsafe { &*frp };
    if fr.fr_layout == crate::buffer_defs::FR_LEAF {
        return std::ptr::eq(fr.fr_win, wp);
    }
    let mut p = fr.fr_child;
    while !p.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { frame_has_win(p, wp) } {
            return true;
        }
        // SAFETY: forwarded from this function's own safety doc.
        p = unsafe { &*p }.fr_next;
    }
    false
}

/// `w_fraction`'s own scale: `w_fraction` ranges from `0` (cursor on
/// the window's first display row) to `FRACTION_MULT` (last row)
/// (`FRACTION_MULT`).
pub const FRACTION_MULT: i32 = 16384;

/// Compute `wp.w_fraction` - the cursor's relative vertical position
/// within the window, on a `0..=`[`FRACTION_MULT`] scale, used to keep
/// the cursor's relative position stable across a window resize
/// (`set_fraction`). A no-op when the window has 1 or fewer display
/// rows (dividing by `w_view_height` would be meaningless there).
pub fn set_fraction(wp: &mut WinT) {
    if wp.w_view_height > 1 {
        // When cursor is in the first line the percentage is computed
        // as if it's halfway that line. Thus with two lines it is
        // 25%, with three lines 17%, etc. Similarly for the last
        // line: 75%, 83%, etc.
        wp.w_fraction =
            (wp.w_wrow * FRACTION_MULT + FRACTION_MULT / 2) / wp.w_view_height;
    }
}

/// Return the frame that should be resized to take over the space
/// occupied by `win` (assumed about to be closed) in tabpage `tp`, or
/// the current tabpage if `tp` is null (`win_altframe`).
///
/// # Safety
/// `win` must be a valid, non-null pointer to a live `WinT`, whose own
/// `w_frame` chain (`fr_prev`/`fr_next`/`fr_parent`) consists of
/// valid, live pointers. If `tp` is non-null, it must be distinct from
/// `GLOBALS.curtab`. Every safety requirement of [`one_window`]/
/// [`alt_tabpage`]/[`frame_fixed_width`]/[`frame_fixed_height`] also
/// applies.
pub unsafe fn win_altframe(
    win: *const WinT,
    tp: *const crate::buffer_defs::TabpageT,
) -> *mut crate::buffer_defs::FrameT {
    debug_assert!(
        tp.is_null() || !std::ptr::eq(tp, unsafe { crate::globals::GLOBALS.get_mut() }.curtab)
    );

    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { one_window(win, tp) } {
        // SAFETY: forwarded from this function's own safety doc.
        let alt_tp = unsafe { alt_tabpage() };
        // SAFETY: forwarded from this function's own safety doc.
        return unsafe { &*(*alt_tp).tp_curwin }.w_frame;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let frp = unsafe { &*win }.w_frame;
    // SAFETY: forwarded from this function's own safety doc.
    let fr = unsafe { &*frp };

    if fr.fr_prev.is_null() {
        return fr.fr_next;
    }
    if fr.fr_next.is_null() {
        return fr.fr_prev;
    }

    // By default the next window will get the space that was
    // abandoned by this window.
    let mut target_fr = fr.fr_next;
    let mut other_fr = fr.fr_prev;

    // SAFETY: forwarded from this function's own safety doc.
    let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
    let (p_sb, p_spr) = (opts.p_sb, opts.p_spr);

    // If this is part of a column of windows and 'splitbelow' is true
    // then the previous window will get the space.
    if !fr.fr_parent.is_null()
        // SAFETY: forwarded from this function's own safety doc.
        && unsafe { &*fr.fr_parent }.fr_layout == crate::buffer_defs::FR_COL
        && p_sb != 0
    {
        target_fr = fr.fr_prev;
        other_fr = fr.fr_next;
    }

    // If this is part of a row of windows, and 'splitright' is true
    // then the previous window will get the space.
    if !fr.fr_parent.is_null()
        // SAFETY: forwarded from this function's own safety doc.
        && unsafe { &*fr.fr_parent }.fr_layout == crate::buffer_defs::FR_ROW
        && p_spr != 0
    {
        target_fr = fr.fr_prev;
        other_fr = fr.fr_next;
    }

    // If 'wfh' or 'wfw' is set for the target and not for the
    // alternate window, reverse the selection.
    if !fr.fr_parent.is_null()
        // SAFETY: forwarded from this function's own safety doc.
        && unsafe { &*fr.fr_parent }.fr_layout == crate::buffer_defs::FR_ROW
    {
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { frame_fixed_width(target_fr) } && !unsafe { frame_fixed_width(other_fr) } {
            target_fr = other_fr;
        }
    // SAFETY: forwarded from this function's own safety doc.
    } else if unsafe { frame_fixed_height(target_fr) } && !unsafe { frame_fixed_height(other_fr) } {
        target_fr = other_fr;
    }

    target_fr
}

/// Build a command string with an optional count suffix appended
/// (e.g. `"quit"` with count `3` becomes `"quit3"`) (`cmd_with_count`).
/// `prenum <= 0` leaves `cmd` unchanged.
#[must_use]
pub fn cmd_with_count(cmd: &[u8], prenum: i64) -> Vec<u8> {
    let mut out = cmd.to_vec();
    if prenum > 0 {
        out.extend_from_slice(prenum.to_string().as_bytes());
    }
    out
}

/// Return the tabpage containing window `win`, or null if not found
/// (`win_find_tabpage`).
///
/// # Safety
/// `crate::globals::GLOBALS.first_tabpage`'s own `tp_next` chain must
/// consist of valid, live `TabpageT` pointers. For each such tabpage,
/// its own window list (`tp_firstwin`/`w_next`, or
/// `GLOBALS.firstwin`/`w_next` when it's the current tabpage) must
/// consist of valid, live `WinT` pointers.
#[must_use]
pub unsafe fn win_find_tabpage(win: *const WinT) -> *mut crate::buffer_defs::TabpageT {
    // SAFETY: forwarded from this function's own safety doc.
    let globals = unsafe { crate::globals::GLOBALS.get_mut() };
    let mut tp = globals.first_tabpage;
    while !tp.is_null() {
        let is_curtab = std::ptr::eq(tp, globals.curtab);
        let mut wp = if is_curtab {
            globals.firstwin
        } else {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { &*tp }.tp_firstwin
        };
        while !wp.is_null() {
            if std::ptr::eq(wp, win) {
                return tp;
            }
            // SAFETY: forwarded from this function's own safety doc.
            wp = unsafe { &*wp }.w_next;
        }
        // SAFETY: forwarded from this function's own safety doc.
        tp = unsafe { &*tp }.tp_next;
    }
    std::ptr::null_mut()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn min_columns_min_lines_status_height_match_c_enum() {
        assert_eq!(MIN_COLUMNS, 12);
        assert_eq!(MIN_LINES, 2);
        assert_eq!(STATUS_HEIGHT, 1);
    }

    #[test]
    fn win_fdccol_count_defaults_to_zero_when_unset() {
        let mut win = WinT::default();
        assert_eq!(unsafe { win_fdccol_count(&mut win) }, 0);
    }

    #[test]
    fn win_fdccol_count_reads_the_configured_digit() {
        let mut win = WinT::default();
        win.w_onebuf_opt.wo_fdc = Some(b"3".to_vec());
        assert_eq!(unsafe { win_fdccol_count(&mut win) }, 3);
    }

    #[test]
    fn win_fdccol_count_auto_is_capped_by_the_real_fold_nesting() {
        // A bare "auto" asks for 1 column, but the cap is the actual
        // nesting depth - with no folds at all that is 0.
        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = WinT { w_buffer: &mut buf, ..Default::default() };
        win.w_onebuf_opt.wo_fdc = Some(b"auto".to_vec());
        assert_eq!(unsafe { win_fdccol_count(&mut win) }, 0);
    }

    #[test]
    fn win_fdccol_count_auto_grants_one_column_for_a_single_fold_level() {
        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = WinT { w_buffer: &mut buf, ..Default::default() };
        win.w_onebuf_opt.wo_fdc = Some(b"auto".to_vec());
        win.w_folds = vec![crate::fold::FoldT { fd_top: 1, fd_len: 3, ..Default::default() }];
        assert_eq!(unsafe { win_fdccol_count(&mut win) }, 1);
    }

    #[test]
    fn win_fdccol_count_auto_n_caps_the_requested_width_at_the_nesting_depth() {
        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = WinT { w_buffer: &mut buf, ..Default::default() };
        // Two levels of nesting, so "auto:5" is capped down to 2
        // while "auto:1" stays at its own smaller request.
        win.w_folds = vec![crate::fold::FoldT {
            fd_top: 1,
            fd_len: 9,
            fd_nested: vec![crate::fold::FoldT { fd_top: 1, fd_len: 3, ..Default::default() }],
            ..Default::default()
        }];
        win.w_onebuf_opt.wo_fdc = Some(b"auto:5".to_vec());
        assert_eq!(unsafe { win_fdccol_count(&mut win) }, 2);
        win.w_onebuf_opt.wo_fdc = Some(b"auto:1".to_vec());
        assert_eq!(unsafe { win_fdccol_count(&mut win) }, 1);
    }

    /// Points `GLOBALS.first_tabpage` at `head` for the guard's
    /// lifetime, restoring the previous value on drop. Callers must
    /// hold `global_state_test_lock()` for the guard's whole lifetime.
    struct FirstTabpageGuard {
        previous: *mut crate::buffer_defs::TabpageT,
    }

    impl FirstTabpageGuard {
        fn set(head: *mut crate::buffer_defs::TabpageT) -> Self {
            let previous = unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage;
            unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage = head;
            FirstTabpageGuard { previous }
        }
    }

    impl Drop for FirstTabpageGuard {
        fn drop(&mut self) {
            unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage = self.previous;
        }
    }

    #[test]
    fn valid_tabpage_true_for_head_of_list() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = FirstTabpageGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);

        assert!(unsafe { valid_tabpage(&tp as *const crate::buffer_defs::TabpageT) });
    }

    #[test]
    fn valid_tabpage_true_for_a_later_list_entry() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tail = crate::buffer_defs::TabpageT::default();
        let mut head = crate::buffer_defs::TabpageT {
            tp_next: &mut tail as *mut crate::buffer_defs::TabpageT,
            ..Default::default()
        };
        let _guard = FirstTabpageGuard::set(&mut head as *mut crate::buffer_defs::TabpageT);

        assert!(unsafe { valid_tabpage(&tail as *const crate::buffer_defs::TabpageT) });
    }

    #[test]
    fn valid_tabpage_false_for_a_pointer_not_in_the_list() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = FirstTabpageGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);

        let stray = crate::buffer_defs::TabpageT::default();
        assert!(!unsafe { valid_tabpage(&stray as *const crate::buffer_defs::TabpageT) });
    }

    #[test]
    fn valid_tabpage_false_for_empty_list() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = FirstTabpageGuard::set(std::ptr::null_mut());

        let stray = crate::buffer_defs::TabpageT::default();
        assert!(!unsafe { valid_tabpage(&stray as *const crate::buffer_defs::TabpageT) });
    }

    // ---- tabpage_index / find_tabpage ----

    #[test]
    fn tabpage_index_finds_head_of_list() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = FirstTabpageGuard::set(tp_ptr);

        assert_eq!(unsafe { tabpage_index(tp_ptr) }, 1);
    }

    #[test]
    fn tabpage_index_finds_a_later_entry() {
        let _lock = crate::globals::global_state_test_lock();
        let mut third = crate::buffer_defs::TabpageT::default();
        let third_ptr = &mut third as *mut crate::buffer_defs::TabpageT;
        let mut second = crate::buffer_defs::TabpageT { tp_next: third_ptr, ..Default::default() };
        let second_ptr = &mut second as *mut crate::buffer_defs::TabpageT;
        let mut first = crate::buffer_defs::TabpageT { tp_next: second_ptr, ..Default::default() };
        let first_ptr = &mut first as *mut crate::buffer_defs::TabpageT;
        let _guard = FirstTabpageGuard::set(first_ptr);

        assert_eq!(unsafe { tabpage_index(first_ptr) }, 1);
        assert_eq!(unsafe { tabpage_index(second_ptr) }, 2);
        assert_eq!(unsafe { tabpage_index(third_ptr) }, 3);
    }

    #[test]
    fn tabpage_index_returns_count_plus_one_when_not_found() {
        let _lock = crate::globals::global_state_test_lock();
        let mut second = crate::buffer_defs::TabpageT::default();
        let mut first =
            crate::buffer_defs::TabpageT { tp_next: &mut second as *mut crate::buffer_defs::TabpageT, ..Default::default() };
        let first_ptr = &mut first as *mut crate::buffer_defs::TabpageT;
        let _guard = FirstTabpageGuard::set(first_ptr);

        // A null pointer never matches any real tab page - same as
        // the original's own tabpagenr("$") = tabpage_index(NULL) - 1
        // idiom (2 tabs -> index 3 -> "$" = 2).
        assert_eq!(unsafe { tabpage_index(std::ptr::null()) }, 3);
    }

    #[test]
    fn find_tabpage_zero_returns_curtab() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let prev_curtab = unsafe { crate::globals::GLOBALS.get_mut() }.curtab;
        unsafe { crate::globals::GLOBALS.get_mut() }.curtab = tp_ptr;

        assert_eq!(unsafe { find_tabpage(0) }, tp_ptr);

        unsafe { crate::globals::GLOBALS.get_mut() }.curtab = prev_curtab;
    }

    #[test]
    fn find_tabpage_finds_by_1_based_number() {
        let _lock = crate::globals::global_state_test_lock();
        let mut second = crate::buffer_defs::TabpageT::default();
        let second_ptr = &mut second as *mut crate::buffer_defs::TabpageT;
        let mut first = crate::buffer_defs::TabpageT { tp_next: second_ptr, ..Default::default() };
        let first_ptr = &mut first as *mut crate::buffer_defs::TabpageT;
        let _guard = FirstTabpageGuard::set(first_ptr);

        assert_eq!(unsafe { find_tabpage(1) }, first_ptr);
        assert_eq!(unsafe { find_tabpage(2) }, second_ptr);
    }

    #[test]
    fn find_tabpage_returns_null_when_out_of_range() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = FirstTabpageGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);

        assert!(unsafe { find_tabpage(99) }.is_null());
    }

    #[test]
    fn handle_get_tabpage_finds_a_matching_handle() {
        let _lock = crate::globals::global_state_test_lock();
        let mut second = crate::buffer_defs::TabpageT { handle: 7, ..Default::default() };
        let second_ptr = &mut second as *mut crate::buffer_defs::TabpageT;
        let mut first =
            crate::buffer_defs::TabpageT { handle: 3, tp_next: second_ptr, ..Default::default() };
        let first_ptr = &mut first as *mut crate::buffer_defs::TabpageT;
        let _guard = FirstTabpageGuard::set(first_ptr);

        assert_eq!(unsafe { handle_get_tabpage(7) }, second_ptr);
        assert_eq!(unsafe { handle_get_tabpage(3) }, first_ptr);
    }

    #[test]
    fn handle_get_tabpage_null_when_not_found() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT { handle: 3, ..Default::default() };
        let _guard = FirstTabpageGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);

        assert!(unsafe { handle_get_tabpage(99) }.is_null());
    }

    #[test]
    fn is_bottom_win_true_for_a_single_top_level_frame() {
        let mut frame = crate::buffer_defs::FrameT::default();
        let win = WinT { w_frame: &mut frame as *mut crate::buffer_defs::FrameT, ..Default::default() };
        assert!(unsafe { is_bottom_win(&win) });
    }

    #[test]
    fn is_bottom_win_false_when_a_col_sibling_frame_follows() {
        // frame is one of two children in a FR_COL (vertically-
        // stacked) parent, with a sibling AFTER it (fr_next != NULL) -
        // meaning there's a window below.
        let mut sibling = crate::buffer_defs::FrameT::default();
        let mut parent = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_COL,
            ..Default::default()
        };
        let mut frame = crate::buffer_defs::FrameT {
            fr_parent: &mut parent as *mut crate::buffer_defs::FrameT,
            fr_next: &mut sibling as *mut crate::buffer_defs::FrameT,
            ..Default::default()
        };
        let win = WinT { w_frame: &mut frame as *mut crate::buffer_defs::FrameT, ..Default::default() };
        assert!(!unsafe { is_bottom_win(&win) });
    }

    #[test]
    fn is_bottom_win_true_when_last_in_a_col_of_frames() {
        // Same FR_COL parent, but frame is the LAST child (fr_next ==
        // NULL) - it's the bottom one.
        let mut parent = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_COL,
            ..Default::default()
        };
        let mut frame = crate::buffer_defs::FrameT {
            fr_parent: &mut parent as *mut crate::buffer_defs::FrameT,
            fr_next: std::ptr::null_mut(),
            ..Default::default()
        };
        let win = WinT { w_frame: &mut frame as *mut crate::buffer_defs::FrameT, ..Default::default() };
        assert!(unsafe { is_bottom_win(&win) });
    }

    #[test]
    fn is_bottom_win_true_when_parent_is_a_row_not_a_column() {
        // A FR_ROW (side-by-side) parent never blocks "bottom" status,
        // regardless of fr_next - only FR_COL siblings matter.
        let mut sibling = crate::buffer_defs::FrameT::default();
        let mut parent = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_ROW,
            ..Default::default()
        };
        let mut frame = crate::buffer_defs::FrameT {
            fr_parent: &mut parent as *mut crate::buffer_defs::FrameT,
            fr_next: &mut sibling as *mut crate::buffer_defs::FrameT,
            ..Default::default()
        };
        let win = WinT { w_frame: &mut frame as *mut crate::buffer_defs::FrameT, ..Default::default() };
        assert!(unsafe { is_bottom_win(&win) });
    }

    #[test]
    fn is_bottom_win_checks_the_whole_ancestor_chain() {
        // frame's own immediate parent is FR_ROW (doesn't block), but
        // the GRANDPARENT is FR_COL with a sibling after the middle
        // frame - still not at the bottom.
        let mut grandparent_sibling = crate::buffer_defs::FrameT::default();
        let mut grandparent = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_COL,
            ..Default::default()
        };
        let mut middle = crate::buffer_defs::FrameT {
            fr_parent: &mut grandparent as *mut crate::buffer_defs::FrameT,
            fr_next: &mut grandparent_sibling as *mut crate::buffer_defs::FrameT,
            fr_layout: crate::buffer_defs::FR_ROW,
            ..Default::default()
        };
        let mut frame = crate::buffer_defs::FrameT {
            fr_parent: &mut middle as *mut crate::buffer_defs::FrameT,
            fr_next: std::ptr::null_mut(),
            ..Default::default()
        };
        let win = WinT { w_frame: &mut frame as *mut crate::buffer_defs::FrameT, ..Default::default() };
        assert!(!unsafe { is_bottom_win(&win) });
    }

    /// Points `GLOBALS.firstwin`/`GLOBALS.curtab` at the given values
    /// for the guard's lifetime, restoring both previous values on
    /// drop. Callers must hold `global_state_test_lock()` for the
    /// guard's whole lifetime (matching `FirstTabpageGuard`'s own
    /// precedent, extended to cover both globals these new functions
    /// touch together).
    struct WindowListGuard {
        prev_firstwin: *mut WinT,
        prev_curtab: *mut crate::buffer_defs::TabpageT,
    }

    impl WindowListGuard {
        fn set(firstwin: *mut WinT, curtab: *mut crate::buffer_defs::TabpageT) -> Self {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            let guard =
                WindowListGuard { prev_firstwin: globals.firstwin, prev_curtab: globals.curtab };
            globals.firstwin = firstwin;
            globals.curtab = curtab;
            guard
        }
    }

    impl Drop for WindowListGuard {
        fn drop(&mut self) {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            globals.firstwin = self.prev_firstwin;
            globals.curtab = self.prev_curtab;
        }
    }

    #[test]
    fn tabpage_win_valid_false_for_null_window() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = WindowListGuard::set(std::ptr::null_mut(), &mut tp);
        assert!(!unsafe { tabpage_win_valid(&tp, std::ptr::null()) });
    }

    #[test]
    fn tabpage_win_valid_true_via_globals_firstwin_when_tp_is_curtab() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT::default();
        let mut tp = crate::buffer_defs::TabpageT::default();
        // GLOBALS.firstwin (NOT tp.tp_firstwin, deliberately left null)
        // is used because tp == curtab.
        let _guard = WindowListGuard::set(&mut win as *mut WinT, &mut tp);
        assert!(unsafe { tabpage_win_valid(&tp, &win) });
    }

    #[test]
    fn tabpage_win_valid_true_via_tp_firstwin_when_tp_is_not_curtab() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT::default();
        let mut other_tp = crate::buffer_defs::TabpageT::default();
        let tp = crate::buffer_defs::TabpageT {
            tp_firstwin: &mut win as *mut WinT,
            ..Default::default()
        };
        // curtab is a DIFFERENT tabpage - tp's own tp_firstwin is used,
        // not GLOBALS.firstwin (left null here).
        let _guard = WindowListGuard::set(std::ptr::null_mut(), &mut other_tp);
        assert!(unsafe { tabpage_win_valid(&tp, &win) });
    }

    #[test]
    fn tabpage_win_valid_false_for_a_window_not_in_the_list() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT::default();
        let stray = WinT::default();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = WindowListGuard::set(&mut win as *mut WinT, &mut tp);
        assert!(!unsafe { tabpage_win_valid(&tp, &stray) });
    }

    #[test]
    fn win_valid_delegates_to_curtab() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT::default();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = WindowListGuard::set(&mut win as *mut WinT, &mut tp);
        assert!(unsafe { win_valid(&win) });

        let stray = WinT::default();
        assert!(!unsafe { win_valid(&stray) });
    }

    #[test]
    fn win_find_by_handle_finds_a_matching_handle_in_the_list() {
        let _lock = crate::globals::global_state_test_lock();
        let mut second = WinT { handle: 7, ..Default::default() };
        let mut first =
            WinT { handle: 3, w_next: &mut second as *mut WinT, ..Default::default() };
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = WindowListGuard::set(&mut first as *mut WinT, &mut tp);

        assert!(std::ptr::eq(unsafe { win_find_by_handle(7) }, &second as *const WinT));
        assert!(std::ptr::eq(unsafe { win_find_by_handle(3) }, &first as *const WinT));
    }

    #[test]
    fn win_find_by_handle_null_when_not_found() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT { handle: 3, ..Default::default() };
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = WindowListGuard::set(&mut win as *mut WinT, &mut tp);

        assert!(unsafe { win_find_by_handle(99) }.is_null());
    }

    #[test]
    fn win_valid_any_tab_finds_a_window_in_a_non_curtab_tabpage() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT::default();
        let mut other_tp = crate::buffer_defs::TabpageT {
            tp_firstwin: &mut win as *mut WinT,
            ..Default::default()
        };
        let mut curtab = crate::buffer_defs::TabpageT {
            tp_next: &mut other_tp as *mut crate::buffer_defs::TabpageT,
            ..Default::default()
        };
        let _first_tabpage_guard =
            FirstTabpageGuard::set(&mut curtab as *mut crate::buffer_defs::TabpageT);
        // GLOBALS.firstwin is empty for curtab itself - win only exists
        // in the SECOND tabpage's own tp_firstwin.
        let _window_list_guard =
            WindowListGuard::set(std::ptr::null_mut(), &mut curtab as *mut _);

        assert!(unsafe { win_valid_any_tab(&win) });
    }

    #[test]
    fn win_valid_any_tab_false_when_null_or_not_found() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _first_tabpage_guard =
            FirstTabpageGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);
        let _window_list_guard = WindowListGuard::set(std::ptr::null_mut(), &mut tp as *mut _);

        assert!(!unsafe { win_valid_any_tab(std::ptr::null()) });
        let stray = WinT::default();
        assert!(!unsafe { win_valid_any_tab(&stray) });
    }

    #[test]
    fn handle_get_window_finds_a_window_in_curtab() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT { handle: 9, ..Default::default() };
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = WindowListGuard::set(&mut win as *mut WinT, &mut tp);
        let _first_tabpage_guard =
            FirstTabpageGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);

        assert!(std::ptr::eq(unsafe { handle_get_window(9) }, &win as *const WinT));
    }

    /// Unlike [`win_find_by_handle`] (deliberately scoped to the
    /// current tab page only, matching the original's own
    /// `win_find_by_handle`), `handle_get_window` finds a window in
    /// ANY tab page, matching the real `handle_get_window` macro's
    /// global-registry semantics.
    #[test]
    fn handle_get_window_finds_a_window_in_a_non_curtab_tabpage() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT { handle: 11, ..Default::default() };
        let mut other_tp = crate::buffer_defs::TabpageT {
            tp_firstwin: &mut win as *mut WinT,
            ..Default::default()
        };
        let mut curtab = crate::buffer_defs::TabpageT {
            tp_next: &mut other_tp as *mut crate::buffer_defs::TabpageT,
            ..Default::default()
        };
        let _first_tabpage_guard =
            FirstTabpageGuard::set(&mut curtab as *mut crate::buffer_defs::TabpageT);
        // GLOBALS.firstwin is empty for curtab itself - win only exists
        // in the SECOND tabpage's own tp_firstwin.
        let _window_list_guard =
            WindowListGuard::set(std::ptr::null_mut(), &mut curtab as *mut _);

        assert!(std::ptr::eq(unsafe { handle_get_window(11) }, &win as *const WinT));
        // The unrelated win_find_by_handle (curtab-only) correctly
        // does NOT find it, confirming the two functions genuinely
        // differ in scope.
        assert!(unsafe { win_find_by_handle(11) }.is_null());
    }

    #[test]
    fn handle_get_window_null_when_not_found() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT { handle: 3, ..Default::default() };
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = WindowListGuard::set(&mut win as *mut WinT, &mut tp);
        let _first_tabpage_guard =
            FirstTabpageGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);

        assert!(unsafe { handle_get_window(99) }.is_null());
    }

    #[test]
    fn win_count_counts_the_current_tabpage_window_list() {
        let _lock = crate::globals::global_state_test_lock();
        let mut third = WinT::default();
        let mut second = WinT { w_next: &mut third as *mut WinT, ..Default::default() };
        let mut first = WinT { w_next: &mut second as *mut WinT, ..Default::default() };
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = WindowListGuard::set(&mut first as *mut WinT, &mut tp);

        assert_eq!(unsafe { win_count() }, 3);
    }

    #[test]
    fn win_count_zero_for_empty_list() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = WindowListGuard::set(std::ptr::null_mut(), &mut tp);

        assert_eq!(unsafe { win_count() }, 0);
    }

    #[test]
    fn check_can_set_curbuf_disabled_true_when_winfixbuf_unset() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT::default();
        win.w_onebuf_opt.wo_wfb = 0;
        let prev_curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = &mut win as *mut WinT;

        assert!(unsafe { check_can_set_curbuf_disabled() });

        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = prev_curwin;
    }

    #[test]
    fn check_can_set_curbuf_disabled_false_when_winfixbuf_set() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT::default();
        win.w_onebuf_opt.wo_wfb = 1;
        let prev_curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = &mut win as *mut WinT;

        assert!(!unsafe { check_can_set_curbuf_disabled() });

        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = prev_curwin;
    }

    #[test]
    fn check_can_set_curbuf_forceit_true_when_forced_even_with_winfixbuf() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT::default();
        win.w_onebuf_opt.wo_wfb = 1;
        let prev_curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = &mut win as *mut WinT;

        assert!(unsafe { check_can_set_curbuf_forceit(true) });

        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = prev_curwin;
    }

    #[test]
    fn check_can_set_curbuf_forceit_false_when_not_forced_and_winfixbuf_set() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT::default();
        win.w_onebuf_opt.wo_wfb = 1;
        let prev_curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = &mut win as *mut WinT;

        assert!(!unsafe { check_can_set_curbuf_forceit(false) });

        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = prev_curwin;
    }

    // ---- window_layout_lock / window_layout_unlock / frames_locked /
    // check_split_disallowed / window_layout_locked ----

    /// Resets `SPLIT_DISALLOWED`/`CLOSE_DISALLOWED`/`FRAME_LOCKED` to
    /// `0` on both construction and drop, so a test using any of the 3
    /// counters can never leak state into a later test regardless of
    /// its own outcome (including a panic).
    struct LayoutLockCountersGuard;
    impl LayoutLockCountersGuard {
        fn reset() -> Self {
            unsafe {
                *SPLIT_DISALLOWED.get_mut() = 0;
                *CLOSE_DISALLOWED.get_mut() = 0;
                *FRAME_LOCKED.get_mut() = 0;
            }
            LayoutLockCountersGuard
        }
    }
    impl Drop for LayoutLockCountersGuard {
        fn drop(&mut self) {
            unsafe {
                *SPLIT_DISALLOWED.get_mut() = 0;
                *CLOSE_DISALLOWED.get_mut() = 0;
                *FRAME_LOCKED.get_mut() = 0;
            }
        }
    }

    #[test]
    fn window_layout_lock_increments_both_counters() {
        let _lock = crate::globals::global_state_test_lock();
        let _counters = LayoutLockCountersGuard::reset();
        window_layout_lock();
        assert_eq!(unsafe { *SPLIT_DISALLOWED.get_mut() }, 1);
        assert_eq!(unsafe { *CLOSE_DISALLOWED.get_mut() }, 1);
        window_layout_lock();
        assert_eq!(unsafe { *SPLIT_DISALLOWED.get_mut() }, 2);
        assert_eq!(unsafe { *CLOSE_DISALLOWED.get_mut() }, 2);
    }

    #[test]
    fn window_layout_unlock_undoes_one_lock_call() {
        let _lock = crate::globals::global_state_test_lock();
        let _counters = LayoutLockCountersGuard::reset();
        window_layout_lock();
        window_layout_lock();
        window_layout_unlock();
        assert_eq!(unsafe { *SPLIT_DISALLOWED.get_mut() }, 1);
        assert_eq!(unsafe { *CLOSE_DISALLOWED.get_mut() }, 1);
    }

    #[test]
    fn frames_locked_false_by_default() {
        let _lock = crate::globals::global_state_test_lock();
        let _counters = LayoutLockCountersGuard::reset();
        assert!(!frames_locked());
    }

    #[test]
    fn frames_locked_true_when_frame_locked_counter_is_nonzero() {
        let _lock = crate::globals::global_state_test_lock();
        let _counters = LayoutLockCountersGuard::reset();
        unsafe { *FRAME_LOCKED.get_mut() = 1 };
        assert!(frames_locked());
    }

    #[test]
    fn check_split_disallowed_err_true_when_nothing_disallows_it() {
        let _lock = crate::globals::global_state_test_lock();
        let _counters = LayoutLockCountersGuard::reset();
        let mut buf = crate::buffer_defs::BufT { b_locked_split: 0, ..Default::default() };
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let win = win_with_buffer(1, buf_ptr);
        assert!(unsafe { check_split_disallowed_err(&win) });
    }

    #[test]
    fn check_split_disallowed_err_false_when_split_disallowed_counter_is_set() {
        let _lock = crate::globals::global_state_test_lock();
        let _counters = LayoutLockCountersGuard::reset();
        window_layout_lock();
        let mut buf = crate::buffer_defs::BufT { b_locked_split: 0, ..Default::default() };
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let win = win_with_buffer(1, buf_ptr);
        assert!(!unsafe { check_split_disallowed_err(&win) });
    }

    #[test]
    fn check_split_disallowed_err_false_when_buffer_has_locked_split() {
        let _lock = crate::globals::global_state_test_lock();
        let _counters = LayoutLockCountersGuard::reset();
        let mut buf = crate::buffer_defs::BufT { b_locked_split: 1, ..Default::default() };
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let win = win_with_buffer(1, buf_ptr);
        assert!(!unsafe { check_split_disallowed_err(&win) });
    }

    #[test]
    fn check_split_disallowed_maps_to_ok_and_fail() {
        let _lock = crate::globals::global_state_test_lock();
        let _counters = LayoutLockCountersGuard::reset();
        let mut buf = crate::buffer_defs::BufT { b_locked_split: 0, ..Default::default() };
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let win = win_with_buffer(1, buf_ptr);
        assert_eq!(unsafe { check_split_disallowed(&win) }, crate::vim_defs::OK);

        window_layout_lock();
        assert_eq!(unsafe { check_split_disallowed(&win) }, crate::vim_defs::FAIL);
    }

    #[test]
    fn window_layout_locked_err_false_when_neither_counter_is_set() {
        let _lock = crate::globals::global_state_test_lock();
        let _counters = LayoutLockCountersGuard::reset();
        assert!(!window_layout_locked_err(crate::ex_cmds_defs::CmdIdxT::split));
    }

    #[test]
    fn window_layout_locked_err_true_when_split_disallowed_is_set() {
        let _lock = crate::globals::global_state_test_lock();
        let _counters = LayoutLockCountersGuard::reset();
        unsafe { *SPLIT_DISALLOWED.get_mut() = 1 };
        assert!(window_layout_locked_err(crate::ex_cmds_defs::CmdIdxT::split));
    }

    #[test]
    fn window_layout_locked_err_true_when_close_disallowed_is_set() {
        let _lock = crate::globals::global_state_test_lock();
        let _counters = LayoutLockCountersGuard::reset();
        unsafe { *CLOSE_DISALLOWED.get_mut() = 1 };
        assert!(window_layout_locked_err(crate::ex_cmds_defs::CmdIdxT::tabnew));
    }

    #[test]
    fn window_layout_locked_matches_window_layout_locked_err() {
        let _lock = crate::globals::global_state_test_lock();
        let _counters = LayoutLockCountersGuard::reset();
        assert!(!window_layout_locked(crate::ex_cmds_defs::CmdIdxT::split));
        window_layout_lock();
        assert!(window_layout_locked(crate::ex_cmds_defs::CmdIdxT::split));
    }

    /// Points `GLOBALS.firstwin`/`GLOBALS.curtab`/`GLOBALS.curwin` at
    /// the given values for the guard's lifetime, restoring all three
    /// previous values on drop - extends `WindowListGuard`'s own
    /// precedent to additionally cover `curwin`, needed by
    /// `win_has_winnr`'s own "is this the tab's current window" check.
    /// Takes `win` ONCE (used for both `firstwin` and `curwin`, since
    /// every real caller wants "the only window IS the current
    /// window") rather than as two separate parameters - deliberately
    /// avoiding a second, independent `&mut win_variable` reborrow at
    /// each call site, which would invalidate the raw pointer already
    /// handed to the first `GLOBALS` field under Stacked Borrows (a
    /// real bug caught here for real by Miri during development; see
    /// `eval/vars.rs`'s own `Box::as_mut()`-then-reborrow precedent for
    /// the same class of bug).
    struct CurwinListGuard {
        prev_firstwin: *mut WinT,
        prev_curtab: *mut crate::buffer_defs::TabpageT,
        prev_curwin: *mut WinT,
    }

    impl CurwinListGuard {
        fn set(win: *mut WinT, tp: *mut crate::buffer_defs::TabpageT) -> Self {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            let guard = CurwinListGuard {
                prev_firstwin: globals.firstwin,
                prev_curtab: globals.curtab,
                prev_curwin: globals.curwin,
            };
            globals.firstwin = win;
            globals.curtab = tp;
            globals.curwin = win;
            guard
        }
    }

    impl Drop for CurwinListGuard {
        fn drop(&mut self) {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            globals.firstwin = self.prev_firstwin;
            globals.curtab = self.prev_curtab;
            globals.curwin = self.prev_curwin;
        }
    }

    fn focusable_win(handle: crate::types_defs::HandleT) -> WinT {
        WinT {
            handle,
            w_config: crate::buffer_defs::WinConfig { focusable: true, hide: false, ..Default::default() },
            ..Default::default()
        }
    }

    // ---- win_has_winnr ----

    #[test]
    fn win_has_winnr_true_for_the_tab_current_window() {
        let _lock = crate::globals::global_state_test_lock();
        // Deliberately NOT focusable/not-hidden - being the current
        // window always counts, regardless of w_config.
        let mut win = WinT { handle: 1, ..Default::default() };
        let mut tp = crate::buffer_defs::TabpageT::default();
        // Compute both raw pointers ONCE, before any guard call, and
        // reuse the SAME pointer values everywhere below - a second,
        // independent `&mut win`/`&mut tp` reborrow after the first
        // has already been handed to a GLOBALS field would invalidate
        // it under Stacked Borrows (see CurwinListGuard's own doc
        // comment for the real bug this avoids).
        let win_ptr = &mut win as *mut WinT;
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = CurwinListGuard::set(win_ptr, tp_ptr);

        assert!(unsafe { win_has_winnr(win_ptr, tp_ptr) });
    }

    #[test]
    fn win_has_winnr_true_for_a_focusable_non_hidden_non_current_window() {
        let _lock = crate::globals::global_state_test_lock();
        let mut curwin = WinT { handle: 1, ..Default::default() };
        let mut other = focusable_win(2);
        let mut tp = crate::buffer_defs::TabpageT::default();
        let curwin_ptr = &mut curwin as *mut WinT;
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = CurwinListGuard::set(curwin_ptr, tp_ptr);

        assert!(unsafe { win_has_winnr(&mut other as *mut WinT, tp_ptr) });
    }

    #[test]
    fn win_has_winnr_false_for_a_hidden_non_current_window() {
        let _lock = crate::globals::global_state_test_lock();
        let mut curwin = WinT { handle: 1, ..Default::default() };
        let mut other = WinT {
            handle: 2,
            w_config: crate::buffer_defs::WinConfig { focusable: true, hide: true, ..Default::default() },
            ..Default::default()
        };
        let mut tp = crate::buffer_defs::TabpageT::default();
        let curwin_ptr = &mut curwin as *mut WinT;
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = CurwinListGuard::set(curwin_ptr, tp_ptr);

        assert!(!unsafe { win_has_winnr(&mut other as *mut WinT, tp_ptr) });
    }

    // ---- win_id2win ----

    #[test]
    fn win_id2win_finds_the_matching_window_number() {
        let _lock = crate::globals::global_state_test_lock();
        let mut third = focusable_win(3);
        let mut second = WinT { w_next: &mut third as *mut WinT, ..focusable_win(2) };
        let mut first = WinT { w_next: &mut second as *mut WinT, ..focusable_win(1) };
        let mut tp = crate::buffer_defs::TabpageT::default();
        let first_ptr = &mut first as *mut WinT;
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = CurwinListGuard::set(first_ptr, tp_ptr);

        assert_eq!(unsafe { win_id2win(1) }, 1);
        assert_eq!(unsafe { win_id2win(2) }, 2);
        assert_eq!(unsafe { win_id2win(3) }, 3);
    }

    #[test]
    fn win_id2win_returns_0_for_an_unknown_handle() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = focusable_win(1);
        let mut tp = crate::buffer_defs::TabpageT::default();
        let win_ptr = &mut win as *mut WinT;
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = CurwinListGuard::set(win_ptr, tp_ptr);

        assert_eq!(unsafe { win_id2win(999) }, 0);
    }

    // ---- win_get_tabwin ----

    #[test]
    fn win_get_tabwin_finds_a_window_in_the_current_tab() {
        let _lock = crate::globals::global_state_test_lock();
        let mut second = focusable_win(2);
        let mut first = WinT { w_next: &mut second as *mut WinT, ..focusable_win(1) };
        let mut tp = crate::buffer_defs::TabpageT::default();
        let first_ptr = &mut first as *mut WinT;
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _lock2 = FirstTabpageGuard::set(tp_ptr);
        let _guard = CurwinListGuard::set(first_ptr, tp_ptr);

        assert_eq!(unsafe { win_get_tabwin(2) }, (1, 2));
    }

    #[test]
    fn win_get_tabwin_finds_a_window_in_a_non_current_tab() {
        let _lock = crate::globals::global_state_test_lock();
        let mut other_win = focusable_win(5);
        let other_win_ptr = &mut other_win as *mut WinT;
        let mut other_tp =
            crate::buffer_defs::TabpageT { tp_firstwin: other_win_ptr, tp_curwin: other_win_ptr, ..Default::default() };
        let mut cur_win = focusable_win(1);
        let other_tp_ptr = &mut other_tp as *mut crate::buffer_defs::TabpageT;
        let mut cur_tp = crate::buffer_defs::TabpageT { tp_next: other_tp_ptr, ..Default::default() };
        // GLOBALS.first_tabpage must chain cur_tp -> other_tp for the
        // walk to find the second tab.
        let cur_win_ptr = &mut cur_win as *mut WinT;
        let cur_tp_ptr = &mut cur_tp as *mut crate::buffer_defs::TabpageT;
        let _lock2 = FirstTabpageGuard::set(cur_tp_ptr);
        let _guard = CurwinListGuard::set(cur_win_ptr, cur_tp_ptr);

        assert_eq!(unsafe { win_get_tabwin(5) }, (2, 1));
    }

    #[test]
    fn win_get_tabwin_returns_0_0_for_an_unknown_handle() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = focusable_win(1);
        let mut tp = crate::buffer_defs::TabpageT::default();
        let win_ptr = &mut win as *mut WinT;
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _lock2 = FirstTabpageGuard::set(tp_ptr);
        let _guard = CurwinListGuard::set(win_ptr, tp_ptr);

        assert_eq!(unsafe { win_get_tabwin(999) }, (0, 0));
    }

    // ---- win_getid ----

    #[test]
    fn win_getid_with_no_args_returns_curwin_handle() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = focusable_win(42);
        let mut tp = crate::buffer_defs::TabpageT::default();
        let win_ptr = &mut win as *mut WinT;
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = CurwinListGuard::set(win_ptr, tp_ptr);

        assert_eq!(unsafe { win_getid(None, None) }, 42);
    }

    #[test]
    fn win_getid_with_winnr_only_uses_current_tab() {
        let _lock = crate::globals::global_state_test_lock();
        let mut second = focusable_win(2);
        let mut first = WinT { w_next: &mut second as *mut WinT, ..focusable_win(1) };
        let mut tp = crate::buffer_defs::TabpageT::default();
        let first_ptr = &mut first as *mut WinT;
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = CurwinListGuard::set(first_ptr, tp_ptr);

        assert_eq!(unsafe { win_getid(Some(2), None) }, 2);
    }

    #[test]
    fn win_getid_with_winnr_0_or_negative_returns_0() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = focusable_win(1);
        let mut tp = crate::buffer_defs::TabpageT::default();
        let win_ptr = &mut win as *mut WinT;
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = CurwinListGuard::set(win_ptr, tp_ptr);

        assert_eq!(unsafe { win_getid(Some(0), None) }, 0);
        assert_eq!(unsafe { win_getid(Some(-1), None) }, 0);
    }

    #[test]
    fn win_getid_with_a_tabnr_that_does_not_exist_returns_minus_1() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = focusable_win(1);
        let mut tp = crate::buffer_defs::TabpageT::default();
        let win_ptr = &mut win as *mut WinT;
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _lock2 = FirstTabpageGuard::set(tp_ptr);
        let _guard = CurwinListGuard::set(win_ptr, tp_ptr);

        assert_eq!(unsafe { win_getid(Some(1), Some(99)) }, -1);
    }

    // ---- win_findbuf ----

    fn win_with_buffer(handle: crate::types_defs::HandleT, buf: *mut crate::buffer_defs::BufT) -> WinT {
        WinT { handle, w_buffer: buf, ..focusable_win(handle) }
    }

    #[test]
    fn win_findbuf_finds_windows_in_the_current_tab() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf_a = crate::buffer_defs::BufT { handle: 10, ..Default::default() };
        let mut buf_b = crate::buffer_defs::BufT { handle: 20, ..Default::default() };
        let buf_a_ptr = &mut buf_a as *mut crate::buffer_defs::BufT;
        let buf_b_ptr = &mut buf_b as *mut crate::buffer_defs::BufT;
        let mut second = win_with_buffer(2, buf_b_ptr);
        let mut first = WinT { w_next: &mut second as *mut WinT, ..win_with_buffer(1, buf_a_ptr) };
        let mut tp = crate::buffer_defs::TabpageT::default();
        let first_ptr = &mut first as *mut WinT;
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _lock2 = FirstTabpageGuard::set(tp_ptr);
        let _guard = CurwinListGuard::set(first_ptr, tp_ptr);

        assert_eq!(unsafe { win_findbuf(10) }, vec![1]);
        assert_eq!(unsafe { win_findbuf(20) }, vec![2]);
    }

    #[test]
    fn win_findbuf_returns_empty_for_an_unknown_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT { handle: 10, ..Default::default() };
        let mut win = win_with_buffer(1, &mut buf as *mut crate::buffer_defs::BufT);
        let mut tp = crate::buffer_defs::TabpageT::default();
        let win_ptr = &mut win as *mut WinT;
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _lock2 = FirstTabpageGuard::set(tp_ptr);
        let _guard = CurwinListGuard::set(win_ptr, tp_ptr);

        assert!(unsafe { win_findbuf(999) }.is_empty());
    }

    #[test]
    fn win_findbuf_finds_a_window_in_a_non_current_tab() {
        let _lock = crate::globals::global_state_test_lock();
        let mut other_buf = crate::buffer_defs::BufT { handle: 30, ..Default::default() };
        let mut other_win = win_with_buffer(7, &mut other_buf as *mut crate::buffer_defs::BufT);
        let other_win_ptr = &mut other_win as *mut WinT;
        let mut other_tp =
            crate::buffer_defs::TabpageT { tp_firstwin: other_win_ptr, tp_curwin: other_win_ptr, ..Default::default() };
        let mut cur_buf = crate::buffer_defs::BufT { handle: 10, ..Default::default() };
        let mut cur_win = win_with_buffer(1, &mut cur_buf as *mut crate::buffer_defs::BufT);
        let other_tp_ptr = &mut other_tp as *mut crate::buffer_defs::TabpageT;
        let mut cur_tp = crate::buffer_defs::TabpageT { tp_next: other_tp_ptr, ..Default::default() };
        let cur_win_ptr = &mut cur_win as *mut WinT;
        let cur_tp_ptr = &mut cur_tp as *mut crate::buffer_defs::TabpageT;
        let _lock2 = FirstTabpageGuard::set(cur_tp_ptr);
        let _guard = CurwinListGuard::set(cur_win_ptr, cur_tp_ptr);

        assert_eq!(unsafe { win_findbuf(30) }, vec![7]);
    }

    // ---- get_winnr ----

    /// Points `GLOBALS.firstwin`/`curtab`/`curwin`/`lastwin`/`prevwin`
    /// at the given values for the guard's lifetime, restoring all
    /// previous values on drop - a `get_winnr`-specific fixture since
    /// (unlike every other function tested above) it needs `firstwin`
    /// and `curwin` to legitimately be TWO DIFFERENT windows (to prove
    /// the counting walk finds a non-head current window).
    struct WinnrGlobalsGuard {
        prev_firstwin: *mut WinT,
        prev_curtab: *mut crate::buffer_defs::TabpageT,
        prev_curwin: *mut WinT,
        prev_lastwin: *mut WinT,
        prev_prevwin: *mut WinT,
    }

    impl WinnrGlobalsGuard {
        fn set(firstwin: *mut WinT, tp: *mut crate::buffer_defs::TabpageT, curwin: *mut WinT, lastwin: *mut WinT, prevwin: *mut WinT) -> Self {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            let guard = WinnrGlobalsGuard {
                prev_firstwin: globals.firstwin,
                prev_curtab: globals.curtab,
                prev_curwin: globals.curwin,
                prev_lastwin: globals.lastwin,
                prev_prevwin: globals.prevwin,
            };
            globals.firstwin = firstwin;
            globals.curtab = tp;
            globals.curwin = curwin;
            globals.lastwin = lastwin;
            globals.prevwin = prevwin;
            guard
        }
    }

    impl Drop for WinnrGlobalsGuard {
        fn drop(&mut self) {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            globals.firstwin = self.prev_firstwin;
            globals.curtab = self.prev_curtab;
            globals.curwin = self.prev_curwin;
            globals.lastwin = self.prev_lastwin;
            globals.prevwin = self.prev_prevwin;
        }
    }

    #[test]
    fn get_winnr_no_arg_returns_the_current_window_number() {
        let _lock = crate::globals::global_state_test_lock();
        let mut third = focusable_win(3);
        let third_ptr = &mut third as *mut WinT;
        let mut second = WinT { w_next: third_ptr, ..focusable_win(2) };
        let second_ptr = &mut second as *mut WinT;
        let mut first = WinT { w_next: second_ptr, ..focusable_win(1) };
        let mut tp = crate::buffer_defs::TabpageT::default();
        let first_ptr = &mut first as *mut WinT;
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = WinnrGlobalsGuard::set(first_ptr, tp_ptr, second_ptr, std::ptr::null_mut(), std::ptr::null_mut());

        assert_eq!(unsafe { get_winnr(tp_ptr, None) }, 2);
    }

    #[test]
    fn get_winnr_dollar_arg_returns_the_last_window_number() {
        let _lock = crate::globals::global_state_test_lock();
        let mut third = focusable_win(3);
        let third_ptr = &mut third as *mut WinT;
        let mut second = WinT { w_next: third_ptr, ..focusable_win(2) };
        let mut first = WinT { w_next: &mut second as *mut WinT, ..focusable_win(1) };
        let mut tp = crate::buffer_defs::TabpageT::default();
        let first_ptr = &mut first as *mut WinT;
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = WinnrGlobalsGuard::set(first_ptr, tp_ptr, first_ptr, third_ptr, std::ptr::null_mut());

        assert_eq!(unsafe { get_winnr(tp_ptr, Some(b"$")) }, 3);
    }

    #[test]
    fn get_winnr_hash_arg_returns_the_previous_window_number() {
        let _lock = crate::globals::global_state_test_lock();
        let mut second = focusable_win(2);
        let second_ptr = &mut second as *mut WinT;
        let mut first = WinT { w_next: second_ptr, ..focusable_win(1) };
        let mut tp = crate::buffer_defs::TabpageT::default();
        let first_ptr = &mut first as *mut WinT;
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = WinnrGlobalsGuard::set(first_ptr, tp_ptr, first_ptr, std::ptr::null_mut(), second_ptr);

        assert_eq!(unsafe { get_winnr(tp_ptr, Some(b"#")) }, 2);
    }

    #[test]
    fn get_winnr_hash_arg_returns_0_when_no_previous_window() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = focusable_win(1);
        let mut tp = crate::buffer_defs::TabpageT::default();
        let win_ptr = &mut win as *mut WinT;
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = WinnrGlobalsGuard::set(win_ptr, tp_ptr, win_ptr, std::ptr::null_mut(), std::ptr::null_mut());

        assert_eq!(unsafe { get_winnr(tp_ptr, Some(b"#")) }, 0);
    }

    #[test]
    fn get_winnr_unrecognized_arg_returns_0() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = focusable_win(1);
        let mut tp = crate::buffer_defs::TabpageT::default();
        let win_ptr = &mut win as *mut WinT;
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = WinnrGlobalsGuard::set(win_ptr, tp_ptr, win_ptr, std::ptr::null_mut(), std::ptr::null_mut());

        assert_eq!(unsafe { get_winnr(tp_ptr, Some(b"xyz")) }, 0);
        assert_eq!(unsafe { get_winnr(tp_ptr, Some(b"3")) }, 0);
    }

    #[test]
    fn get_winnr_digit_direction_form_navigates_to_a_real_neighbor() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win1 = focusable_win(1);
        let mut win2 = focusable_win(2);
        let win1_ptr = &mut win1 as *mut WinT;
        let win2_ptr = &mut win2 as *mut WinT;
        let mut leaf2 = crate::buffer_defs::FrameT { fr_win: win2_ptr, ..Default::default() };
        let leaf2_ptr = &mut leaf2 as *mut crate::buffer_defs::FrameT;
        let mut leaf1 =
            crate::buffer_defs::FrameT { fr_win: win1_ptr, fr_next: leaf2_ptr, ..Default::default() };
        let leaf1_ptr = &mut leaf1 as *mut crate::buffer_defs::FrameT;
        // SAFETY: `leaf2_ptr`/`leaf1_ptr` are valid, live pointers into
        // this test's own locals.
        unsafe { (*leaf2_ptr).fr_prev = leaf1_ptr };
        let mut col = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_COL,
            fr_child: leaf1_ptr,
            ..Default::default()
        };
        let col_ptr = &mut col as *mut crate::buffer_defs::FrameT;
        // SAFETY: forwarded from the earlier comment.
        unsafe {
            (*leaf1_ptr).fr_parent = col_ptr;
            (*leaf2_ptr).fr_parent = col_ptr;
            (*win1_ptr).w_frame = leaf1_ptr;
            (*win2_ptr).w_frame = leaf2_ptr;
            // Separate, plain window-LIST linkage (distinct from the
            // frame-tree linkage above) - needed by get_winnr's own
            // trailing "count up to twin" walk from `firstwin`.
            (*win1_ptr).w_next = win2_ptr;
        }
        let mut tp = crate::buffer_defs::TabpageT { tp_topframe: col_ptr, ..Default::default() };
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = WinnrGlobalsGuard::set(win1_ptr, tp_ptr, win1_ptr, std::ptr::null_mut(), std::ptr::null_mut());

        // From win1 (top), "1j" (down) reaches win2.
        assert_eq!(unsafe { get_winnr(tp_ptr, Some(b"1j")) }, 2);
        // From win1 (top), "1k" (up) has no neighbor - stays on win1.
        assert_eq!(unsafe { get_winnr(tp_ptr, Some(b"1k")) }, 1);
    }

    #[test]
    fn win_vert_neighbor_returns_wp_itself_when_no_neighbor_exists() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = focusable_win(1);
        let win_ptr = &mut win as *mut WinT;
        let mut leaf = crate::buffer_defs::FrameT { fr_win: win_ptr, ..Default::default() };
        let leaf_ptr = &mut leaf as *mut crate::buffer_defs::FrameT;
        // SAFETY: `win_ptr` is a valid, live pointer into this test's
        // own local.
        unsafe { (*win_ptr).w_frame = leaf_ptr };
        let tp = crate::buffer_defs::TabpageT { tp_topframe: leaf_ptr, ..Default::default() };

        let found = unsafe { win_vert_neighbor(&tp, win_ptr, true, 1) };
        assert_eq!(found, win_ptr);
    }

    #[test]
    fn win_horz_neighbor_finds_the_right_neighbor_in_a_row_split() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win1 = focusable_win(1);
        let mut win2 = focusable_win(2);
        let win1_ptr = &mut win1 as *mut WinT;
        let win2_ptr = &mut win2 as *mut WinT;
        let mut leaf2 = crate::buffer_defs::FrameT { fr_win: win2_ptr, ..Default::default() };
        let leaf2_ptr = &mut leaf2 as *mut crate::buffer_defs::FrameT;
        let mut leaf1 =
            crate::buffer_defs::FrameT { fr_win: win1_ptr, fr_next: leaf2_ptr, ..Default::default() };
        let leaf1_ptr = &mut leaf1 as *mut crate::buffer_defs::FrameT;
        // SAFETY: `leaf1_ptr`/`leaf2_ptr` are valid, live pointers into
        // this test's own locals.
        unsafe { (*leaf2_ptr).fr_prev = leaf1_ptr };
        let mut row = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_ROW,
            fr_child: leaf1_ptr,
            ..Default::default()
        };
        let row_ptr = &mut row as *mut crate::buffer_defs::FrameT;
        // SAFETY: forwarded from the earlier comment.
        unsafe {
            (*leaf1_ptr).fr_parent = row_ptr;
            (*leaf2_ptr).fr_parent = row_ptr;
            (*win1_ptr).w_frame = leaf1_ptr;
            (*win2_ptr).w_frame = leaf2_ptr;
        }
        let tp = crate::buffer_defs::TabpageT { tp_topframe: row_ptr, ..Default::default() };

        let found = unsafe { win_horz_neighbor(&tp, win1_ptr, false, 1) };
        assert_eq!(found, win2_ptr);
    }

    #[test]
    fn frame2win_walks_down_to_the_leaf() {
        let mut win = focusable_win(1);
        let win_ptr = &mut win as *mut WinT;
        let leaf = crate::buffer_defs::FrameT { fr_win: win_ptr, ..Default::default() };
        let leaf_ptr = &leaf as *const crate::buffer_defs::FrameT as *mut crate::buffer_defs::FrameT;
        let root =
            crate::buffer_defs::FrameT { fr_layout: crate::buffer_defs::FR_ROW, fr_child: leaf_ptr, ..Default::default() };
        assert_eq!(unsafe { frame2win(&root) }, win_ptr);
    }

    // ---- frame_fixed_height / frame_fixed_width ----

    /// Builds a `WinT` with a specific `'winfixheight'`/`'winfixwidth'`
    /// value pre-set.
    fn win_with_fixed(wfh: i32, wfw: i32) -> WinT {
        let mut w = focusable_win(1);
        w.w_onebuf_opt.wo_wfh = wfh;
        w.w_onebuf_opt.wo_wfw = wfw;
        w
    }

    #[test]
    fn frame_fixed_height_leaf_reflects_the_window_option() {
        let mut fixed_win = win_with_fixed(1, 0);
        let fixed_win_ptr = &mut fixed_win as *mut WinT;
        let fixed_leaf = crate::buffer_defs::FrameT { fr_win: fixed_win_ptr, ..Default::default() };
        assert!(unsafe { frame_fixed_height(&fixed_leaf) });

        let mut free_win = win_with_fixed(0, 0);
        let free_win_ptr = &mut free_win as *mut WinT;
        let free_leaf = crate::buffer_defs::FrameT { fr_win: free_win_ptr, ..Default::default() };
        assert!(!unsafe { frame_fixed_height(&free_leaf) });
    }

    #[test]
    fn frame_fixed_height_row_is_true_if_any_child_is_fixed() {
        let mut fixed_win = win_with_fixed(1, 0);
        let fixed_win_ptr = &mut fixed_win as *mut WinT;
        let mut free_win = win_with_fixed(0, 0);
        let free_win_ptr = &mut free_win as *mut WinT;
        let mut leaf2 = crate::buffer_defs::FrameT { fr_win: free_win_ptr, ..Default::default() };
        let leaf2_ptr = &mut leaf2 as *mut crate::buffer_defs::FrameT;
        let leaf1 =
            crate::buffer_defs::FrameT { fr_win: fixed_win_ptr, fr_next: leaf2_ptr, ..Default::default() };
        let row = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_ROW,
            fr_child: &leaf1 as *const _ as *mut crate::buffer_defs::FrameT,
            ..Default::default()
        };
        assert!(unsafe { frame_fixed_height(&row) });
    }

    #[test]
    fn frame_fixed_height_row_is_false_if_no_child_is_fixed() {
        let mut free_win1 = win_with_fixed(0, 0);
        let free_win1_ptr = &mut free_win1 as *mut WinT;
        let mut free_win2 = win_with_fixed(0, 0);
        let free_win2_ptr = &mut free_win2 as *mut WinT;
        let mut leaf2 = crate::buffer_defs::FrameT { fr_win: free_win2_ptr, ..Default::default() };
        let leaf2_ptr = &mut leaf2 as *mut crate::buffer_defs::FrameT;
        let leaf1 =
            crate::buffer_defs::FrameT { fr_win: free_win1_ptr, fr_next: leaf2_ptr, ..Default::default() };
        let row = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_ROW,
            fr_child: &leaf1 as *const _ as *mut crate::buffer_defs::FrameT,
            ..Default::default()
        };
        assert!(!unsafe { frame_fixed_height(&row) });
    }

    #[test]
    fn frame_fixed_height_col_needs_every_child_fixed() {
        let mut fixed_win = win_with_fixed(1, 0);
        let fixed_win_ptr = &mut fixed_win as *mut WinT;
        let mut free_win = win_with_fixed(0, 0);
        let free_win_ptr = &mut free_win as *mut WinT;

        // One fixed, one not: FR_COL as a whole is NOT fixed.
        let mut leaf2 = crate::buffer_defs::FrameT { fr_win: free_win_ptr, ..Default::default() };
        let leaf2_ptr = &mut leaf2 as *mut crate::buffer_defs::FrameT;
        let mixed_leaf1 =
            crate::buffer_defs::FrameT { fr_win: fixed_win_ptr, fr_next: leaf2_ptr, ..Default::default() };
        let mixed_col = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_COL,
            fr_child: &mixed_leaf1 as *const _ as *mut crate::buffer_defs::FrameT,
            ..Default::default()
        };
        assert!(!unsafe { frame_fixed_height(&mixed_col) });

        // Both fixed: FR_COL as a whole IS fixed.
        let mut fixed_win2 = win_with_fixed(1, 0);
        let fixed_win2_ptr = &mut fixed_win2 as *mut WinT;
        let mut both_leaf2 = crate::buffer_defs::FrameT { fr_win: fixed_win2_ptr, ..Default::default() };
        let both_leaf2_ptr = &mut both_leaf2 as *mut crate::buffer_defs::FrameT;
        let both_leaf1 = crate::buffer_defs::FrameT {
            fr_win: fixed_win_ptr,
            fr_next: both_leaf2_ptr,
            ..Default::default()
        };
        let both_col = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_COL,
            fr_child: &both_leaf1 as *const _ as *mut crate::buffer_defs::FrameT,
            ..Default::default()
        };
        assert!(unsafe { frame_fixed_height(&both_col) });
    }

    #[test]
    fn frame_fixed_width_leaf_reflects_the_window_option() {
        let mut fixed_win = win_with_fixed(0, 1);
        let fixed_win_ptr = &mut fixed_win as *mut WinT;
        let fixed_leaf = crate::buffer_defs::FrameT { fr_win: fixed_win_ptr, ..Default::default() };
        assert!(unsafe { frame_fixed_width(&fixed_leaf) });

        let mut free_win = win_with_fixed(0, 0);
        let free_win_ptr = &mut free_win as *mut WinT;
        let free_leaf = crate::buffer_defs::FrameT { fr_win: free_win_ptr, ..Default::default() };
        assert!(!unsafe { frame_fixed_width(&free_leaf) });
    }

    #[test]
    fn frame_fixed_width_col_is_true_if_any_child_is_fixed() {
        // FR_COL's "any" role is the OPPOSITE of frame_fixed_height's
        // own FR_ROW-is-"any" - verifying the roles are genuinely
        // swapped, not accidentally identical to frame_fixed_height.
        let mut fixed_win = win_with_fixed(0, 1);
        let fixed_win_ptr = &mut fixed_win as *mut WinT;
        let mut free_win = win_with_fixed(0, 0);
        let free_win_ptr = &mut free_win as *mut WinT;
        let mut leaf2 = crate::buffer_defs::FrameT { fr_win: free_win_ptr, ..Default::default() };
        let leaf2_ptr = &mut leaf2 as *mut crate::buffer_defs::FrameT;
        let leaf1 =
            crate::buffer_defs::FrameT { fr_win: fixed_win_ptr, fr_next: leaf2_ptr, ..Default::default() };
        let col = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_COL,
            fr_child: &leaf1 as *const _ as *mut crate::buffer_defs::FrameT,
            ..Default::default()
        };
        assert!(unsafe { frame_fixed_width(&col) });
    }

    #[test]
    fn frame_fixed_width_row_needs_every_child_fixed() {
        let mut fixed_win = win_with_fixed(0, 1);
        let fixed_win_ptr = &mut fixed_win as *mut WinT;
        let mut free_win = win_with_fixed(0, 0);
        let free_win_ptr = &mut free_win as *mut WinT;
        let mut leaf2 = crate::buffer_defs::FrameT { fr_win: free_win_ptr, ..Default::default() };
        let leaf2_ptr = &mut leaf2 as *mut crate::buffer_defs::FrameT;
        let leaf1 =
            crate::buffer_defs::FrameT { fr_win: fixed_win_ptr, fr_next: leaf2_ptr, ..Default::default() };
        let row = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_ROW,
            fr_child: &leaf1 as *const _ as *mut crate::buffer_defs::FrameT,
            ..Default::default()
        };
        assert!(!unsafe { frame_fixed_width(&row) });
    }

    // ---- frame_fix_width / frame_fix_height ----

    #[test]
    fn frame_fix_width_sets_frame_width_from_window() {
        let mut frame = crate::buffer_defs::FrameT { fr_width: 0, ..Default::default() };
        let frame_ptr = &mut frame as *mut crate::buffer_defs::FrameT;
        let mut win = WinT { w_width: 30, w_vsep_width: 1, w_frame: frame_ptr, ..Default::default() };
        let win_ptr = &mut win as *mut WinT;
        unsafe { frame_fix_width(win_ptr) };
        assert_eq!(unsafe { &*frame_ptr }.fr_width, 31);
    }

    #[test]
    fn frame_fix_width_with_zero_vsep_matches_window_width_exactly() {
        let mut frame = crate::buffer_defs::FrameT { fr_width: 999, ..Default::default() };
        let frame_ptr = &mut frame as *mut crate::buffer_defs::FrameT;
        let mut win = WinT { w_width: 80, w_vsep_width: 0, w_frame: frame_ptr, ..Default::default() };
        let win_ptr = &mut win as *mut WinT;
        unsafe { frame_fix_width(win_ptr) };
        assert_eq!(unsafe { &*frame_ptr }.fr_width, 80);
    }

    #[test]
    fn frame_fix_height_sets_frame_height_from_window() {
        let mut frame = crate::buffer_defs::FrameT { fr_height: 0, ..Default::default() };
        let frame_ptr = &mut frame as *mut crate::buffer_defs::FrameT;
        let mut win = WinT {
            w_height: 20,
            w_hsep_height: 1,
            w_status_height: 1,
            w_frame: frame_ptr,
            ..Default::default()
        };
        let win_ptr = &mut win as *mut WinT;
        unsafe { frame_fix_height(win_ptr) };
        assert_eq!(unsafe { &*frame_ptr }.fr_height, 22);
    }

    #[test]
    fn frame_fix_height_with_zero_hsep_and_status_matches_window_height_exactly() {
        let mut frame = crate::buffer_defs::FrameT { fr_height: 999, ..Default::default() };
        let frame_ptr = &mut frame as *mut crate::buffer_defs::FrameT;
        let mut win = WinT {
            w_height: 24,
            w_hsep_height: 0,
            w_status_height: 0,
            w_frame: frame_ptr,
            ..Default::default()
        };
        let win_ptr = &mut win as *mut WinT;
        unsafe { frame_fix_height(win_ptr) };
        assert_eq!(unsafe { &*frame_ptr }.fr_height, 24);
    }

    // ---- frame_minheight / frame_minwidth ----

    /// RAII guard temporarily setting `OPTION_VARS.p_wh`/`p_wmh`/
    /// `p_wiw`/`p_wmw`, restoring the previous values on drop. Caller
    /// must hold `global_state_test_lock()` for the whole lifetime.
    struct MinSizeOptsGuard {
        prev_wh: crate::types_defs::OptInt,
        prev_wmh: crate::types_defs::OptInt,
        prev_wiw: crate::types_defs::OptInt,
        prev_wmw: crate::types_defs::OptInt,
    }
    impl MinSizeOptsGuard {
        fn set(wh: i64, wmh: i64, wiw: i64, wmw: i64) -> Self {
            let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            let guard = MinSizeOptsGuard {
                prev_wh: opts.p_wh,
                prev_wmh: opts.p_wmh,
                prev_wiw: opts.p_wiw,
                prev_wmw: opts.p_wmw,
            };
            opts.p_wh = wh;
            opts.p_wmh = wmh;
            opts.p_wiw = wiw;
            opts.p_wmw = wmw;
            guard
        }
    }
    impl Drop for MinSizeOptsGuard {
        fn drop(&mut self) {
            let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            opts.p_wh = self.prev_wh;
            opts.p_wmh = self.prev_wmh;
            opts.p_wiw = self.prev_wiw;
            opts.p_wmw = self.prev_wmw;
        }
    }

    fn win_with_extras(handle: crate::types_defs::HandleT) -> WinT {
        WinT {
            handle,
            w_winbar_height: 1,
            w_hsep_height: 2,
            w_status_height: 3,
            w_vsep_width: 4,
            ..Default::default()
        }
    }

    #[test]
    fn frame_minheight_leaf_uses_winheight_for_next_curwin() {
        let _lock = crate::globals::global_state_test_lock();
        let _opts = MinSizeOptsGuard::set(10, 2, 20, 5);
        let mut win = win_with_extras(1);
        let win_ptr = &mut win as *mut WinT;
        let leaf = crate::buffer_defs::FrameT { fr_win: win_ptr, ..Default::default() };
        // extra_height = 1 + 2 + 3 = 6; p_wh(10) + 6 = 16.
        assert_eq!(unsafe { frame_minheight(&leaf, win_ptr) }, 16);
    }

    #[test]
    fn frame_minheight_leaf_uses_winminheight_for_a_non_current_window() {
        let _lock = crate::globals::global_state_test_lock();
        let _opts = MinSizeOptsGuard::set(10, 2, 20, 5);
        let mut win = win_with_extras(1);
        let mut other = win_with_extras(2);
        let win_ptr = &mut win as *mut WinT;
        let other_ptr = &mut other as *mut WinT;
        let _guard = CurwinListGuard::set(other_ptr, std::ptr::null_mut());
        let leaf = crate::buffer_defs::FrameT { fr_win: win_ptr, ..Default::default() };
        // Not next_curwin, not curwin: p_wmh(2) + extra_height(6) = 8.
        assert_eq!(unsafe { frame_minheight(&leaf, other_ptr) }, 8);
    }

    #[test]
    fn frame_minheight_current_window_gets_a_plus_one_bump_when_winminheight_is_zero() {
        let _lock = crate::globals::global_state_test_lock();
        let _opts = MinSizeOptsGuard::set(10, 0, 20, 5);
        let mut win = win_with_extras(1);
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurwinListGuard::set(win_ptr, std::ptr::null_mut());
        let leaf = crate::buffer_defs::FrameT { fr_win: win_ptr, ..Default::default() };
        // curwin, next_curwin == NULL, p_wmh == 0: 0 + 6 + 1 = 7.
        assert_eq!(unsafe { frame_minheight(&leaf, std::ptr::null_mut()) }, 7);
    }

    #[test]
    fn frame_minheight_current_window_no_bump_when_winminheight_is_nonzero() {
        let _lock = crate::globals::global_state_test_lock();
        let _opts = MinSizeOptsGuard::set(10, 3, 20, 5);
        let mut win = win_with_extras(1);
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurwinListGuard::set(win_ptr, std::ptr::null_mut());
        let leaf = crate::buffer_defs::FrameT { fr_win: win_ptr, ..Default::default() };
        // p_wmh == 3 (nonzero): no +1 bump. 3 + 6 = 9.
        assert_eq!(unsafe { frame_minheight(&leaf, std::ptr::null_mut()) }, 9);
    }

    #[test]
    fn frame_minheight_nowin_suppresses_the_current_window_bump() {
        let _lock = crate::globals::global_state_test_lock();
        let _opts = MinSizeOptsGuard::set(10, 0, 20, 5);
        let mut win = win_with_extras(1);
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurwinListGuard::set(win_ptr, std::ptr::null_mut());
        let leaf = crate::buffer_defs::FrameT { fr_win: win_ptr, ..Default::default() };
        // NOWIN is non-null, so the "next_curwin.is_null()" bump-gate
        // is false even though this IS curwin and p_wmh == 0.
        assert_eq!(unsafe { frame_minheight(&leaf, NOWIN) }, 6);
    }

    #[test]
    fn frame_minheight_row_takes_the_maximum_of_its_children() {
        let _lock = crate::globals::global_state_test_lock();
        let _opts = MinSizeOptsGuard::set(10, 2, 20, 5);
        let mut win1 = win_with_extras(1); // minheight (not curwin/next): 2+6=8
        let mut win2 = WinT { handle: 2, w_winbar_height: 0, w_hsep_height: 0, w_status_height: 0, ..Default::default() }; // 2+0=2
        let win1_ptr = &mut win1 as *mut WinT;
        let win2_ptr = &mut win2 as *mut WinT;
        let leaf2 = crate::buffer_defs::FrameT { fr_win: win2_ptr, ..Default::default() };
        let leaf2_ptr = &leaf2 as *const _ as *mut crate::buffer_defs::FrameT;
        let leaf1 =
            crate::buffer_defs::FrameT { fr_win: win1_ptr, fr_next: leaf2_ptr, ..Default::default() };
        let row = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_ROW,
            fr_child: &leaf1 as *const _ as *mut crate::buffer_defs::FrameT,
            ..Default::default()
        };
        assert_eq!(unsafe { frame_minheight(&row, std::ptr::null_mut()) }, 8);
    }

    #[test]
    fn frame_minheight_col_sums_its_children() {
        let _lock = crate::globals::global_state_test_lock();
        let _opts = MinSizeOptsGuard::set(10, 2, 20, 5);
        let mut win1 = win_with_extras(1); // 2+6=8
        let mut win2 = WinT { handle: 2, w_winbar_height: 0, w_hsep_height: 0, w_status_height: 0, ..Default::default() }; // 2+0=2
        let win1_ptr = &mut win1 as *mut WinT;
        let win2_ptr = &mut win2 as *mut WinT;
        let leaf2 = crate::buffer_defs::FrameT { fr_win: win2_ptr, ..Default::default() };
        let leaf2_ptr = &leaf2 as *const _ as *mut crate::buffer_defs::FrameT;
        let leaf1 =
            crate::buffer_defs::FrameT { fr_win: win1_ptr, fr_next: leaf2_ptr, ..Default::default() };
        let col = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_COL,
            fr_child: &leaf1 as *const _ as *mut crate::buffer_defs::FrameT,
            ..Default::default()
        };
        assert_eq!(unsafe { frame_minheight(&col, std::ptr::null_mut()) }, 10); // 8 + 2
    }

    #[test]
    fn frame_minwidth_leaf_uses_winwidth_for_next_curwin() {
        let _lock = crate::globals::global_state_test_lock();
        let _opts = MinSizeOptsGuard::set(10, 2, 20, 5);
        let mut win = win_with_extras(1);
        let win_ptr = &mut win as *mut WinT;
        let leaf = crate::buffer_defs::FrameT { fr_win: win_ptr, ..Default::default() };
        // p_wiw(20) + w_vsep_width(4) = 24.
        assert_eq!(unsafe { frame_minwidth(&leaf, win_ptr) }, 24);
    }

    #[test]
    fn frame_minwidth_current_window_gets_a_plus_one_bump_when_winminwidth_is_zero() {
        let _lock = crate::globals::global_state_test_lock();
        let _opts = MinSizeOptsGuard::set(10, 2, 20, 0);
        let mut win = win_with_extras(1);
        let win_ptr = &mut win as *mut WinT;
        let _guard = CurwinListGuard::set(win_ptr, std::ptr::null_mut());
        let leaf = crate::buffer_defs::FrameT { fr_win: win_ptr, ..Default::default() };
        // 0 + 4 + 1 = 5.
        assert_eq!(unsafe { frame_minwidth(&leaf, std::ptr::null_mut()) }, 5);
    }

    #[test]
    fn frame_minwidth_col_is_the_any_case_taking_the_maximum() {
        // FR_COL's role for width is the "max" case, the OPPOSITE of
        // frame_minheight's own FR_ROW-is-"max" - verifying the roles
        // are genuinely swapped, matching frame_fixed_width's own
        // swapped ROW/COL convention.
        let _lock = crate::globals::global_state_test_lock();
        let _opts = MinSizeOptsGuard::set(10, 2, 20, 5);
        let mut win1 = win_with_extras(1); // p_wmw(5) + w_vsep_width(4) = 9
        let mut win2 = WinT { handle: 2, w_vsep_width: 0, ..Default::default() }; // 5 + 0 = 5
        let win1_ptr = &mut win1 as *mut WinT;
        let win2_ptr = &mut win2 as *mut WinT;
        let leaf2 = crate::buffer_defs::FrameT { fr_win: win2_ptr, ..Default::default() };
        let leaf2_ptr = &leaf2 as *const _ as *mut crate::buffer_defs::FrameT;
        let leaf1 =
            crate::buffer_defs::FrameT { fr_win: win1_ptr, fr_next: leaf2_ptr, ..Default::default() };
        let col = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_COL,
            fr_child: &leaf1 as *const _ as *mut crate::buffer_defs::FrameT,
            ..Default::default()
        };
        assert_eq!(unsafe { frame_minwidth(&col, std::ptr::null_mut()) }, 9);
    }

    #[test]
    fn frame_minwidth_row_sums_its_children() {
        let _lock = crate::globals::global_state_test_lock();
        let _opts = MinSizeOptsGuard::set(10, 2, 20, 5);
        let mut win1 = win_with_extras(1); // 9
        let mut win2 = WinT { handle: 2, w_vsep_width: 0, ..Default::default() }; // 5
        let win1_ptr = &mut win1 as *mut WinT;
        let win2_ptr = &mut win2 as *mut WinT;
        let leaf2 = crate::buffer_defs::FrameT { fr_win: win2_ptr, ..Default::default() };
        let leaf2_ptr = &leaf2 as *const _ as *mut crate::buffer_defs::FrameT;
        let leaf1 =
            crate::buffer_defs::FrameT { fr_win: win1_ptr, fr_next: leaf2_ptr, ..Default::default() };
        let row = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_ROW,
            fr_child: &leaf1 as *const _ as *mut crate::buffer_defs::FrameT,
            ..Default::default()
        };
        assert_eq!(unsafe { frame_minwidth(&row, std::ptr::null_mut()) }, 14); // 9 + 5
    }

    // ---- win_default_scroll / tabline_height / global_stl_height / min_rows(_for_all_tabpages) ----

    #[test]
    fn win_default_scroll_halves_the_view_height() {
        let win = WinT { w_view_height: 20, ..Default::default() };
        assert_eq!(unsafe { win_default_scroll(&win) }, 10);
    }

    #[test]
    fn win_default_scroll_never_returns_less_than_one() {
        let win = WinT { w_view_height: 0, ..Default::default() };
        assert_eq!(unsafe { win_default_scroll(&win) }, 1);
        let win2 = WinT { w_view_height: 1, ..Default::default() };
        assert_eq!(unsafe { win_default_scroll(&win2) }, 1);
    }

    /// RAII guard temporarily setting `OPTION_VARS.p_stal`/`p_ls` and
    /// `GLOBALS.first_tabpage`, restoring the previous values on drop.
    /// Caller must hold `global_state_test_lock()`.
    struct TablineGlobalsGuard {
        prev_stal: crate::types_defs::OptInt,
        prev_ls: crate::types_defs::OptInt,
        prev_first_tabpage: *mut crate::buffer_defs::TabpageT,
    }
    impl TablineGlobalsGuard {
        fn set(stal: i64, ls: i64, first_tabpage: *mut crate::buffer_defs::TabpageT) -> Self {
            let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            let guard = TablineGlobalsGuard {
                prev_stal: opts.p_stal,
                prev_ls: opts.p_ls,
                prev_first_tabpage: globals.first_tabpage,
            };
            opts.p_stal = stal;
            opts.p_ls = ls;
            globals.first_tabpage = first_tabpage;
            guard
        }
    }
    impl Drop for TablineGlobalsGuard {
        fn drop(&mut self) {
            let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            opts.p_stal = self.prev_stal;
            opts.p_ls = self.prev_ls;
            globals.first_tabpage = self.prev_first_tabpage;
        }
    }

    #[test]
    fn tabline_height_zero_when_showtabline_is_zero() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = TablineGlobalsGuard::set(0, 0, tp_ptr);
        assert_eq!(unsafe { tabline_height() }, 0);
    }

    // --- snapshot_windows_scroll_size ---

    #[test]
    fn snapshot_windows_scroll_size_copies_every_window_in_the_chain() {
        let _lock = crate::globals::global_state_test_lock();
        let mut second = crate::buffer_defs::WinT {
            w_topline: 9,
            w_topfill: 2,
            w_leftcol: 3,
            w_skipcol: 4,
            w_width: 40,
            w_height: 12,
            ..Default::default()
        };
        let second_ptr: *mut WinT = &mut second;
        let mut first = crate::buffer_defs::WinT {
            w_topline: 5,
            w_topfill: 1,
            w_leftcol: 6,
            w_skipcol: 7,
            w_width: 80,
            w_height: 24,
            w_next: second_ptr,
            ..Default::default()
        };
        let first_ptr: *mut WinT = &mut first;
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev = g.firstwin;
        g.firstwin = first_ptr;

        unsafe { snapshot_windows_scroll_size() };

        unsafe {
            assert_eq!((*first_ptr).w_last_topline, 5);
            assert_eq!((*first_ptr).w_last_topfill, 1);
            assert_eq!((*first_ptr).w_last_leftcol, 6);
            assert_eq!((*first_ptr).w_last_skipcol, 7);
            assert_eq!((*first_ptr).w_last_width, 80);
            assert_eq!((*first_ptr).w_last_height, 24);
            // The second window in the chain is reached too.
            assert_eq!((*second_ptr).w_last_topline, 9);
            assert_eq!((*second_ptr).w_last_width, 40);
            assert_eq!((*second_ptr).w_last_height, 12);
        }

        unsafe { crate::globals::GLOBALS.get_mut() }.firstwin = prev;
    }

    #[test]
    fn snapshot_windows_scroll_size_on_an_empty_window_list_is_a_noop() {
        let _lock = crate::globals::global_state_test_lock();
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev = g.firstwin;
        g.firstwin = std::ptr::null_mut();

        unsafe { snapshot_windows_scroll_size() };

        unsafe { crate::globals::GLOBALS.get_mut() }.firstwin = prev;
    }

    // --- rows_avail / win_init_size / prevwin_curwin ---

    #[test]
    fn rows_avail_subtracts_the_cmdline_tabline_and_global_statusline() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_rows = g.Rows;
        g.Rows = 30;

        {
            // 'showtabline'=0, 'laststatus'=0: only the command line.
            let _guard = TablineGlobalsGuard::set(0, 0, tp_ptr);
            let p_ch = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ch as i32;
            assert_eq!(unsafe { rows_avail() }, 30 - p_ch);
        }
        {
            // 'showtabline'=2 always shows it, 'laststatus'=3 adds a
            // global status line - one row each.
            let _guard = TablineGlobalsGuard::set(2, 3, tp_ptr);
            let p_ch = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ch as i32;
            assert_eq!(unsafe { rows_avail() }, 30 - p_ch - 1 - STATUS_HEIGHT);
        }

        unsafe { crate::globals::GLOBALS.get_mut() }.Rows = prev_rows;
    }

    #[test]
    fn may_make_initial_scroll_size_snapshot_only_fires_once() {
        // A one-shot guard: only the FIRST call snapshots, so a later
        // call cannot overwrite the baseline WinScrolled compares
        // against. The flag is shared, so it is saved and restored.
        let _lock = crate::globals::global_state_test_lock();
        let prev_done = *unsafe { DID_INITIAL_SCROLL_SIZE_SNAPSHOT.get_mut() };
        let prev_first = unsafe { crate::globals::GLOBALS.get_mut() }.firstwin;

        let mut win = crate::buffer_defs::WinT {
            w_topline: 42,
            w_last_topline: 0,
            ..Default::default()
        };
        unsafe { crate::globals::GLOBALS.get_mut() }.firstwin = &mut win;
        *unsafe { DID_INITIAL_SCROLL_SIZE_SNAPSHOT.get_mut() } = false;

        // First call takes the snapshot.
        unsafe { may_make_initial_scroll_size_snapshot() };
        assert_eq!(win.w_last_topline, 42);
        assert!(*unsafe { DID_INITIAL_SCROLL_SIZE_SNAPSHOT.get_mut() });

        // A later move must NOT be picked up by a second call.
        win.w_topline = 99;
        unsafe { may_make_initial_scroll_size_snapshot() };
        assert_eq!(win.w_last_topline, 42, "the second call must be a no-op");

        *unsafe { DID_INITIAL_SCROLL_SIZE_SNAPSHOT.get_mut() } = prev_done;
        unsafe { crate::globals::GLOBALS.get_mut() }.firstwin = prev_first;
    }

    #[test]
    fn find_horizontally_resizable_frame_returns_a_frame_with_room() {
        // A frame taller than its minimum can give up a line, so it is
        // returned immediately without any walking.
        let _lock = crate::globals::global_state_test_lock();
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let (prev_wmh, prev_wh) = (opts.p_wmh, opts.p_wh);
        opts.p_wmh = 1;
        opts.p_wh = 1;
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let (prev_top, prev_cur) = (g.topframe, g.curwin);
        g.curwin = std::ptr::null_mut();

        let mut win = crate::buffer_defs::WinT::default();
        let mut top = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_LEAF,
            fr_win: &mut win,
            fr_height: 10,
            ..Default::default()
        };
        unsafe { crate::globals::GLOBALS.get_mut() }.topframe = &mut top;

        let got = unsafe { find_horizontally_resizable_frame(&mut top) };
        assert!(std::ptr::eq(got, &raw mut top));

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.topframe = prev_top;
        g.curwin = prev_cur;
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        opts.p_wmh = prev_wmh;
        opts.p_wh = prev_wh;
    }

    #[test]
    fn find_horizontally_resizable_frame_is_null_when_topframe_is_minimal() {
        // Reaching the top frame with no room left means nothing can
        // be resized at all.
        let _lock = crate::globals::global_state_test_lock();
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let (prev_wmh, prev_wh) = (opts.p_wmh, opts.p_wh);
        opts.p_wmh = 1;
        opts.p_wh = 1;
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let (prev_top, prev_cur) = (g.topframe, g.curwin);
        g.curwin = std::ptr::null_mut();

        let mut win = crate::buffer_defs::WinT::default();
        let mut top = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_LEAF,
            fr_win: &mut win,
            // Exactly at the minimum, so it cannot give up a line.
            fr_height: 1,
            ..Default::default()
        };
        unsafe { crate::globals::GLOBALS.get_mut() }.topframe = &mut top;

        assert!(unsafe { find_horizontally_resizable_frame(&mut top) }.is_null());

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.topframe = prev_top;
        g.curwin = prev_cur;
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        opts.p_wmh = prev_wmh;
        opts.p_wh = prev_wh;
    }

    #[test]
    fn find_horizontally_resizable_frame_steps_to_the_frame_above_in_a_column() {
        // Inside a COLUMN the walk goes to the frame ABOVE (fr_prev),
        // since that one shares the vertical space - not straight up
        // to the parent.
        let _lock = crate::globals::global_state_test_lock();
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let (prev_wmh, prev_wh) = (opts.p_wmh, opts.p_wh);
        opts.p_wmh = 1;
        opts.p_wh = 1;
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let (prev_top, prev_cur) = (g.topframe, g.curwin);
        g.curwin = std::ptr::null_mut();

        let mut win_above = crate::buffer_defs::WinT::default();
        let mut win_below = crate::buffer_defs::WinT::default();

        let mut parent = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_COL,
            fr_height: 20,
            ..Default::default()
        };
        // The frame above has room to shrink.
        let mut above = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_LEAF,
            fr_win: &mut win_above,
            fr_height: 10,
            fr_parent: &mut parent,
            ..Default::default()
        };
        // The starting frame is already minimal.
        let mut below = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_LEAF,
            fr_win: &mut win_below,
            fr_height: 1,
            fr_parent: &mut parent,
            fr_prev: &mut above,
            ..Default::default()
        };
        unsafe { crate::globals::GLOBALS.get_mut() }.topframe = &mut parent;

        let got = unsafe { find_horizontally_resizable_frame(&mut below) };
        assert!(std::ptr::eq(got, &raw mut above), "must step to the frame ABOVE");

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.topframe = prev_top;
        g.curwin = prev_cur;
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        opts.p_wmh = prev_wmh;
        opts.p_wh = prev_wh;
    }

    #[test]
    fn win_size_save_records_a_total_then_two_entries_per_window() {
        // The FIRST entry is the total lines available for windows,
        // not a window size - a later restore compares it against the
        // value at that time. Each window then contributes exactly two
        // entries: width-plus-separator, then height.
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = TablineGlobalsGuard::set(0, 0, tp_ptr);

        let mut second = crate::buffer_defs::WinT {
            w_width: 30,
            w_vsep_width: 0,
            w_height: 9,
            ..Default::default()
        };
        let mut first = crate::buffer_defs::WinT {
            w_width: 40,
            w_vsep_width: 1,
            w_height: 10,
            w_next: &mut second,
            ..Default::default()
        };

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let (prev_rows, prev_first) = (g.Rows, g.firstwin);
        g.Rows = 30;
        g.firstwin = &mut first;

        let sizes = unsafe { win_size_save() };

        // 1 total + 2 windows x 2 entries.
        assert_eq!(sizes.len(), 5);
        // Width INCLUDES the vertical separator.
        assert_eq!(sizes[1], 41);
        assert_eq!(sizes[2], 10);
        assert_eq!(sizes[3], 30);
        assert_eq!(sizes[4], 9);

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.Rows = prev_rows;
        g.firstwin = prev_first;
    }

    #[test]
    fn win_size_save_with_a_single_window_has_three_entries() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = TablineGlobalsGuard::set(0, 0, tp_ptr);

        let mut only = crate::buffer_defs::WinT {
            w_width: 80,
            w_height: 24,
            ..Default::default()
        };
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let (prev_rows, prev_first) = (g.Rows, g.firstwin);
        g.Rows = 30;
        g.firstwin = &mut only;

        let sizes = unsafe { win_size_save() };
        assert_eq!(sizes.len(), 3);
        assert_eq!(sizes[1], 80);
        assert_eq!(sizes[2], 24);

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.Rows = prev_rows;
        g.firstwin = prev_first;
    }

    #[test]
    fn clear_snapshot_frees_the_slot_and_is_safe_to_repeat() {
        // Clearing the slot is what makes a second call safe: the
        // recursive free would otherwise walk an already-freed tree.
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let leaf = Box::into_raw(Box::new(crate::buffer_defs::FrameT::default()));
        tp.tp_snapshot[0] = Box::into_raw(Box::new(crate::buffer_defs::FrameT {
            fr_child: leaf,
            ..Default::default()
        }));

        unsafe { clear_snapshot(&mut tp, 0) };
        assert!(tp.tp_snapshot[0].is_null());

        // A second clear must be a harmless no-op.
        unsafe { clear_snapshot(&mut tp, 0) };
        assert!(tp.tp_snapshot[0].is_null());
    }

    #[test]
    fn get_snapshot_curwin_is_null_for_an_empty_slot() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let prev_tab = unsafe { crate::globals::GLOBALS.get_mut() }.curtab;
        unsafe { crate::globals::GLOBALS.get_mut() }.curtab = &mut tp;

        assert!(unsafe { get_snapshot_curwin(0) }.is_null());

        unsafe { crate::globals::GLOBALS.get_mut() }.curtab = prev_tab;
    }

    #[test]
    fn get_snapshot_curwin_finds_the_recorded_window() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let mut win = crate::buffer_defs::WinT::default();

        // Populate the snapshot BEFORE installing tp in GLOBALS: a
        // later write through `tp` directly would invalidate the
        // pointer already derived from it, which Miri (Tree Borrows)
        // rejects. Same hazard option.rs documents for optval_from_varp.
        tp.tp_snapshot[1] = Box::into_raw(Box::new(crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_LEAF,
            fr_win: &mut win,
            ..Default::default()
        }));

        let prev_tab = unsafe { crate::globals::GLOBALS.get_mut() }.curtab;
        unsafe { crate::globals::GLOBALS.get_mut() }.curtab = &mut tp;

        let got = unsafe { get_snapshot_curwin(1) };
        assert!(std::ptr::eq(got, &raw mut win));

        // Restore curtab first, so the cleanup's own &mut tp has no
        // live derived pointer to invalidate.
        unsafe { crate::globals::GLOBALS.get_mut() }.curtab = prev_tab;
        unsafe { clear_snapshot(&mut tp, 1) };
    }

    #[test]
    fn make_snapshot_rec_copies_the_layout_and_only_the_curwin_leaf() {
        // Only the layout shape and sizes are recorded. The single
        // leaf holding curwin stores that pointer; every other leaf's
        // fr_win stays null, which is what get_snapshot_curwin_rec's
        // fallthrough relies on.
        let _lock = crate::globals::global_state_test_lock();
        let mut cur = crate::buffer_defs::WinT::default();
        let mut other = crate::buffer_defs::WinT::default();
        let prev_curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = &mut cur;

        let mut leaf_other = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_LEAF,
            fr_win: &mut other,
            fr_width: 11,
            fr_height: 7,
            ..Default::default()
        };
        let mut leaf_cur = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_LEAF,
            fr_win: &mut cur,
            fr_next: &mut leaf_other,
            fr_width: 22,
            fr_height: 9,
            ..Default::default()
        };
        let mut root = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_ROW,
            fr_child: &mut leaf_cur,
            fr_width: 33,
            fr_height: 16,
            ..Default::default()
        };

        let snap = unsafe { make_snapshot_rec(&mut root) };
        // SAFETY: make_snapshot_rec just built this tree.
        unsafe {
            assert_eq!((*snap).fr_layout, crate::buffer_defs::FR_ROW);
            assert_eq!((*snap).fr_width, 33);
            assert_eq!((*snap).fr_height, 16);
            // The root is not a leaf, so it records no window.
            assert!((*snap).fr_win.is_null());

            let child = (*snap).fr_child;
            assert!(!child.is_null());
            assert_eq!((*child).fr_width, 22);
            // This leaf held curwin, so the pointer IS recorded.
            assert!(std::ptr::eq((*child).fr_win, &raw mut cur));

            let sibling = (*child).fr_next;
            assert!(!sibling.is_null());
            assert_eq!((*sibling).fr_width, 11);
            // This leaf held a different window, so it records none.
            assert!((*sibling).fr_win.is_null());

            clear_snapshot_rec(snap);
        }

        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = prev_curwin;
    }

    #[test]
    fn check_snapshot_rec_accepts_a_matching_layout() {
        let _lock = crate::globals::global_state_test_lock();
        let mut leaf = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_LEAF,
            ..Default::default()
        };
        let mut root = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_ROW,
            fr_child: &mut leaf,
            ..Default::default()
        };
        let snap = unsafe { make_snapshot_rec(&mut root) };
        assert_eq!(unsafe { check_snapshot_rec(snap, &mut root) }, crate::vim_defs::OK);
        unsafe { clear_snapshot_rec(snap) };
    }

    #[test]
    fn check_snapshot_rec_rejects_a_changed_layout() {
        let _lock = crate::globals::global_state_test_lock();
        let mut leaf = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_LEAF,
            ..Default::default()
        };
        let mut root = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_ROW,
            fr_child: &mut leaf,
            ..Default::default()
        };
        let snap = unsafe { make_snapshot_rec(&mut root) };

        // A different frame KIND is a mismatch.
        root.fr_layout = crate::buffer_defs::FR_COL;
        assert_eq!(unsafe { check_snapshot_rec(snap, &mut root) }, crate::vim_defs::FAIL);
        root.fr_layout = crate::buffer_defs::FR_ROW;

        // So is losing a child.
        root.fr_child = std::ptr::null_mut();
        assert_eq!(unsafe { check_snapshot_rec(snap, &mut root) }, crate::vim_defs::FAIL);

        unsafe { clear_snapshot_rec(snap) };
    }

    #[test]
    fn get_snapshot_curwin_rec_prefers_siblings_then_children() {
        // Siblings are searched before children, and a node's own
        // fr_win is only used when neither yielded a window - so the
        // deepest, last-visited branch wins.
        let mut win_sibling = crate::buffer_defs::WinT::default();
        let mut win_child = crate::buffer_defs::WinT::default();
        let mut win_root = crate::buffer_defs::WinT::default();

        let mut sibling = crate::buffer_defs::FrameT {
            fr_win: &mut win_sibling,
            ..Default::default()
        };
        let mut child = crate::buffer_defs::FrameT {
            fr_win: &mut win_child,
            ..Default::default()
        };
        let mut root = crate::buffer_defs::FrameT {
            fr_next: &mut sibling,
            fr_child: &mut child,
            fr_win: &mut win_root,
            ..Default::default()
        };

        // The sibling is checked first, so its window is the answer.
        let got = unsafe { get_snapshot_curwin_rec(&mut root) };
        assert!(std::ptr::eq(got, &raw mut win_sibling));

        // Without a sibling, the child is used.
        root.fr_next = std::ptr::null_mut();
        let got = unsafe { get_snapshot_curwin_rec(&mut root) };
        assert!(std::ptr::eq(got, &raw mut win_child));

        // With neither, the node's own window is the fallback.
        root.fr_child = std::ptr::null_mut();
        let got = unsafe { get_snapshot_curwin_rec(&mut root) };
        assert!(std::ptr::eq(got, &raw mut win_root));
    }

    #[test]
    fn get_snapshot_curwin_rec_falls_through_interior_frames() {
        // An interior frame's fr_win is null, which is exactly what
        // makes the "keep looking" fallthrough work: a null result
        // from a subtree must not stop the search.
        let mut win_deep = crate::buffer_defs::WinT::default();
        let mut deep = crate::buffer_defs::FrameT {
            fr_win: &mut win_deep,
            ..Default::default()
        };
        // Interior node: no window of its own, only a child.
        let mut interior = crate::buffer_defs::FrameT {
            fr_child: &mut deep,
            ..Default::default()
        };
        let mut root = crate::buffer_defs::FrameT {
            fr_child: &mut interior,
            ..Default::default()
        };

        let got = unsafe { get_snapshot_curwin_rec(&mut root) };
        assert!(std::ptr::eq(got, &raw mut win_deep));
    }

    #[test]
    fn clear_snapshot_rec_frees_a_whole_tree() {
        // Build a real heap-allocated tree the same way a snapshot
        // would, then free it. Under Miri (without ignore-leaks) this
        // proves every node is reclaimed exactly once.
        let leaf = Box::into_raw(Box::new(crate::buffer_defs::FrameT::default()));
        let sibling = Box::into_raw(Box::new(crate::buffer_defs::FrameT::default()));
        let root = Box::into_raw(Box::new(crate::buffer_defs::FrameT {
            fr_child: leaf,
            fr_next: sibling,
            ..Default::default()
        }));

        unsafe { clear_snapshot_rec(root) };
    }

    #[test]
    fn clear_snapshot_rec_of_null_is_a_no_op() {
        unsafe { clear_snapshot_rec(std::ptr::null_mut()) };
    }

    /// `win_remove_status_line` calls `comp_col`, which reaches
    /// `last_stl_height` and dereferences the GLOBAL `firstwin`.
    /// Without setting it, the walk follows whatever window an earlier
    /// test left behind - which segfaults once that window's storage
    /// is gone. This installs `win` as the one and only window for the
    /// duration.
    struct StlGuard {
        prev_firstwin: *mut crate::buffer_defs::WinT,
        prev_lastwin: *mut crate::buffer_defs::WinT,
        prev_curwin: *mut crate::buffer_defs::WinT,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl StlGuard {
        fn set(win: *mut crate::buffer_defs::WinT) -> Self {
            let _lock = crate::globals::global_state_test_lock();
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            let guard = StlGuard {
                prev_firstwin: g.firstwin,
                prev_lastwin: g.lastwin,
                prev_curwin: g.curwin,
                _lock,
            };
            unsafe {
                (*win).w_next = std::ptr::null_mut();
                (*win).w_prev = std::ptr::null_mut();
            }
            g.firstwin = win;
            g.lastwin = win;
            g.curwin = win;
            guard
        }
    }

    impl Drop for StlGuard {
        fn drop(&mut self) {
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            g.firstwin = self.prev_firstwin;
            g.lastwin = self.prev_lastwin;
            g.curwin = self.prev_curwin;
        }
    }

    #[test]
    fn win_remove_status_line_gives_the_height_back_to_the_window() {
        // Boxed, not stack-allocated: these pointers are installed
        // into GLOBALS, so they need stable heap addresses.
        //
        // w_view_height is pre-set to the height the window will HAVE
        // after the status line is reclaimed. `win_new_height` reaches
        // `win_set_inner_size`, which still has an `unimplemented!()`
        // boundary for a genuine height CHANGE (it needs
        // validate_cursor/set_fraction/win_comp_scroll/
        // scroll_to_fraction). Lining the two up exercises everything
        // this function itself does without crossing that boundary.
        let mut buf = Box::new(crate::buffer_defs::BufT::default());
        let buf_ptr = std::ptr::addr_of_mut!(*buf);
        let mut win = Box::new(crate::buffer_defs::WinT {
            w_status_height: 1,
            w_height: 10,
            w_view_height: 10 + STATUS_HEIGHT,
            w_buffer: buf_ptr,
            ..Default::default()
        });
        win.w_status_click_defs.push(crate::statusline_defs::StlClickDefinition::default());
        let win_ptr = std::ptr::addr_of_mut!(*win);
        let _guard = StlGuard::set(win_ptr);

        unsafe { win_remove_status_line(win_ptr, false) };

        assert_eq!(win.w_status_height, 0);
        assert_eq!(win.w_height, 10 + STATUS_HEIGHT, "the freed line goes back");
        assert_eq!(win.w_hsep_height, 0, "no separator was asked for");
        assert!(win.w_status_click_defs.is_empty());
    }

    #[test]
    fn win_remove_status_line_can_put_a_separator_in_its_place() {
        // With add_hsep the height is NOT given back - a horizontal
        // separator occupies the row instead, so win_new_height is
        // never called at all on this branch.
        let mut buf = Box::new(crate::buffer_defs::BufT::default());
        let buf_ptr = std::ptr::addr_of_mut!(*buf);
        let mut win = Box::new(crate::buffer_defs::WinT {
            w_status_height: 1,
            w_height: 10,
            w_buffer: buf_ptr,
            ..Default::default()
        });
        let win_ptr = std::ptr::addr_of_mut!(*win);
        let _guard = StlGuard::set(win_ptr);

        unsafe { win_remove_status_line(win_ptr, true) };

        assert_eq!(win.w_status_height, 0);
        assert_eq!(win.w_hsep_height, 1);
        assert_eq!(win.w_height, 10, "height is unchanged in this branch");
    }

    #[test]
    fn win_remove_status_line_uses_the_view_height_for_a_floating_window() {
        // A floating window's own w_height is not the height the
        // status line came out of; w_view_height is. The winbar height
        // absorbs the difference so win_set_inner_size still sees no
        // net change (see the first test's own comment).
        let mut buf = Box::new(crate::buffer_defs::BufT::default());
        let buf_ptr = std::ptr::addr_of_mut!(*buf);
        let mut win = Box::new(crate::buffer_defs::WinT {
            w_status_height: 1,
            w_floating: true,
            w_height: 3,
            w_view_height: 8,
            w_winbar_height: STATUS_HEIGHT,
            w_buffer: buf_ptr,
            ..Default::default()
        });
        let win_ptr = std::ptr::addr_of_mut!(*win);
        let _guard = StlGuard::set(win_ptr);

        unsafe { win_remove_status_line(win_ptr, false) };

        assert_eq!(
            win.w_height,
            8 + STATUS_HEIGHT,
            "the new height comes from w_view_height, not w_height"
        );
    }

    #[test]
    fn curwin_init_resets_the_current_window() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = crate::buffer_defs::WinT {
            w_lines_valid: 9,
            w_topline: 40,
            w_buffer: &mut buf,
            ..Default::default()
        };
        win.w_cursor.lnum = 40;
        let win_ptr = std::ptr::addr_of_mut!(win);

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev = g.curwin;
        g.curwin = win_ptr;

        unsafe { curwin_init() };

        assert_eq!(win.w_lines_valid, 0);
        assert_eq!(win.w_topline, 1);
        assert_eq!(win.w_cursor.lnum, 1);

        unsafe { crate::globals::GLOBALS.get_mut() }.curwin = prev;
    }

    #[test]
    fn win_init_empty_resets_the_view_to_the_top() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = crate::buffer_defs::WinT {
            w_lines_valid: 9,
            w_curswant: 5,
            w_topline: 40,
            w_topfill: 3,
            w_botline: 99,
            w_valid: 0xff,
            w_buffer: &mut buf,
            ..Default::default()
        };
        win.w_cursor.lnum = 40;
        win.w_cursor.col = 7;
        win.w_cursor.coladd = 2;
        win.w_pcmark.lnum = 40;
        win.w_pcmark.col = 7;
        win.w_prev_pcmark.lnum = 30;
        win.w_prev_pcmark.col = 3;

        unsafe { win_init_empty(&mut win) };

        assert_eq!(win.w_lines_valid, 0);
        assert_eq!(win.w_cursor.lnum, 1);
        assert_eq!(win.w_cursor.col, 0);
        assert_eq!(win.w_cursor.coladd, 0);
        assert_eq!(win.w_curswant, 0);
        assert_eq!(win.w_topline, 1);
        assert_eq!(win.w_topfill, 0);
        assert_eq!(win.w_botline, 2);
        assert_eq!(win.w_valid, 0);
        // w_s points at the buffer's own syntax block.
        assert!(std::ptr::eq(win.w_s, &raw mut buf.b_s));
    }

    #[test]
    fn win_init_empty_sets_pcmark_to_line_one_but_clears_prev_pcmark() {
        // The asymmetry is deliberate and called out in the original's
        // own comment: pcmark is SET to line 1, not cleared, while
        // prev_pcmark really does go to line 0.
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT::default();
        let mut win = crate::buffer_defs::WinT { w_buffer: &mut buf, ..Default::default() };
        win.w_pcmark.lnum = 40;
        win.w_pcmark.col = 7;
        win.w_prev_pcmark.lnum = 30;
        win.w_prev_pcmark.col = 3;

        unsafe { win_init_empty(&mut win) };

        assert_eq!(win.w_pcmark.lnum, 1);
        assert_eq!(win.w_pcmark.col, 0);
        assert_eq!(win.w_prev_pcmark.lnum, 0);
        assert_eq!(win.w_prev_pcmark.col, 0);
    }

    #[test]
    fn new_frame_gives_the_window_a_leaf_frame_pointing_back_at_it() {
        let mut win = crate::buffer_defs::WinT::default();
        let wp = &raw mut win;
        unsafe { new_frame(wp) };

        assert!(!win.w_frame.is_null());
        // SAFETY: new_frame just allocated this frame.
        let frp = unsafe { &*win.w_frame };
        assert_eq!(frp.fr_layout, crate::buffer_defs::FR_LEAF);
        assert!(std::ptr::eq(frp.fr_win, wp));

        // Reclaim what new_frame allocated - the real caller's window
        // teardown does this.
        // SAFETY: the frame came from Box::into_raw in new_frame.
        drop(unsafe { Box::from_raw(win.w_frame) });
    }

    #[test]
    fn win_init_size_fills_the_screen_with_the_first_window() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = TablineGlobalsGuard::set(0, 0, tp_ptr);

        let mut win = crate::buffer_defs::WinT { w_winbar_height: 1, ..Default::default() };
        let mut frame = crate::buffer_defs::FrameT::default();
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let (pr, pc, pf, pt) = (g.Rows, g.Columns, g.firstwin, g.topframe);
        g.Rows = 30;
        g.Columns = 80;
        g.firstwin = &mut win;
        g.topframe = &mut frame;

        let avail = unsafe { rows_avail() };
        unsafe { win_init_size() };

        assert_eq!(win.w_height, avail);
        assert_eq!(win.w_prev_height, avail);
        assert_eq!(win.w_height_outer, avail);
        assert_eq!(frame.fr_height, avail);
        // The winbar eats into the VIEW height, not the total.
        assert_eq!(win.w_view_height, avail - 1);
        assert_eq!(win.w_winrow_off, 1);
        assert_eq!((win.w_width, win.w_view_width, win.w_width_outer), (80, 80, 80));
        assert_eq!(frame.fr_width, 80);

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.Rows = pr;
        g.Columns = pc;
        g.firstwin = pf;
        g.topframe = pt;
    }

    #[test]
    fn prevwin_curwin_is_curwin_outside_the_cmdline_window() {
        let _lock = crate::globals::global_state_test_lock();
        let mut cur = crate::buffer_defs::WinT::default();
        let mut prev = crate::buffer_defs::WinT::default();
        let (cur_ptr, prev_ptr): (*mut WinT, *mut WinT) = (&mut cur, &mut prev);
        // is_in_cmdwin() reaches curbuf via bt_cmdwin, so it has to be
        // a real buffer; a default one is not a command-line window.
        let mut buf = crate::buffer_defs::BufT::default();
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let (pc, pp, pb) = (g.curwin, g.prevwin, g.curbuf);
        g.curwin = cur_ptr;
        g.prevwin = prev_ptr;
        g.curbuf = &mut buf;

        // is_in_cmdwin() is false with no command-line window open, so
        // prevwin is ignored even though it is set.
        assert!(std::ptr::eq(unsafe { prevwin_curwin() }, cur_ptr));

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.curwin = pc;
        g.prevwin = pp;
        g.curbuf = pb;
    }

    #[test]
    fn tabline_height_one_only_shown_with_multiple_tabs() {
        let _lock = crate::globals::global_state_test_lock();
        let mut only_tab = crate::buffer_defs::TabpageT::default();
        let only_tab_ptr = &mut only_tab as *mut crate::buffer_defs::TabpageT;
        {
            let _guard = TablineGlobalsGuard::set(1, 0, only_tab_ptr);
            assert_eq!(unsafe { tabline_height() }, 0);
        }

        let mut second_tab = crate::buffer_defs::TabpageT::default();
        let second_tab_ptr = &mut second_tab as *mut crate::buffer_defs::TabpageT;
        let mut first_tab =
            crate::buffer_defs::TabpageT { tp_next: second_tab_ptr, ..Default::default() };
        let first_tab_ptr = &mut first_tab as *mut crate::buffer_defs::TabpageT;
        let _guard = TablineGlobalsGuard::set(1, 0, first_tab_ptr);
        assert_eq!(unsafe { tabline_height() }, 1);
    }

    #[test]
    fn tabline_height_always_one_when_showtabline_is_two() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = TablineGlobalsGuard::set(2, 0, tp_ptr);
        assert_eq!(unsafe { tabline_height() }, 1);
    }

    #[test]
    fn global_stl_height_one_only_when_laststatus_is_three() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = TablineGlobalsGuard::set(0, 3, tp_ptr);
        assert_eq!(unsafe { global_stl_height() }, STATUS_HEIGHT);

        let _guard2 = TablineGlobalsGuard::set(0, 2, tp_ptr);
        assert_eq!(unsafe { global_stl_height() }, 0);
    }

    #[test]
    fn min_rows_not_initialized_yet_returns_min_lines() {
        let _lock = crate::globals::global_state_test_lock();
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_firstwin = globals.firstwin;
        globals.firstwin = std::ptr::null_mut();
        let tp = crate::buffer_defs::TabpageT::default();
        assert_eq!(unsafe { min_rows(&tp) }, MIN_LINES);
        unsafe { crate::globals::GLOBALS.get_mut() }.firstwin = prev_firstwin;
    }

    #[test]
    fn min_rows_for_all_tabpages_not_initialized_yet_returns_min_lines() {
        let _lock = crate::globals::global_state_test_lock();
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_firstwin = globals.firstwin;
        globals.firstwin = std::ptr::null_mut();
        assert_eq!(unsafe { min_rows_for_all_tabpages() }, MIN_LINES);
        unsafe { crate::globals::GLOBALS.get_mut() }.firstwin = prev_firstwin;
    }

    #[test]
    fn min_rows_combines_frame_minheight_tabline_and_statusline() {
        let _lock = crate::globals::global_state_test_lock();
        let _opts = MinSizeOptsGuard::set(10, 2, 20, 5);
        let mut win = win_with_extras(1); // minheight (not curwin/next): p_wmh(2)+extra(6)=8
        let win_ptr = &mut win as *mut WinT;
        let leaf = crate::buffer_defs::FrameT { fr_win: win_ptr, ..Default::default() };
        let mut tp = crate::buffer_defs::TabpageT {
            tp_topframe: &leaf as *const _ as *mut crate::buffer_defs::FrameT,
            tp_ch_used: 0, // not curtab's own p_ch, and 0 here means no +1 bump
            ..Default::default()
        };
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_firstwin = globals.firstwin;
        let prev_curtab = globals.curtab;
        let prev_first_tabpage = globals.first_tabpage;
        globals.firstwin = win_ptr; // just needs to be non-null
        globals.curtab = std::ptr::null_mut(); // tp itself is NOT curtab
        // tabline_height (called internally by min_rows) asserts
        // first_tabpage is non-null - it need not be tp itself.
        globals.first_tabpage = tp_ptr;
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        opts.p_stal = 0; // tabline_height == 0
        opts.p_ls = 0; // global_stl_height == 0

        // 8 (frame_minheight) + 0 (tabline) + 0 (statusline) + 0 (no
        // command-line room, since tp_ch_used == 0) = 8.
        assert_eq!(unsafe { min_rows(tp_ptr) }, 8);

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        globals.firstwin = prev_firstwin;
        globals.curtab = prev_curtab;
        globals.first_tabpage = prev_first_tabpage;
    }

    #[test]
    fn min_rows_for_all_tabpages_takes_the_maximum_across_tabs() {
        let _lock = crate::globals::global_state_test_lock();
        let _opts = MinSizeOptsGuard::set(10, 2, 20, 5);
        let mut win1 = win_with_extras(1); // 2+6=8
        let mut win2 = WinT { handle: 2, w_winbar_height: 0, w_hsep_height: 0, w_status_height: 0, ..Default::default() }; // 2+0=2
        let win1_ptr = &mut win1 as *mut WinT;
        let win2_ptr = &mut win2 as *mut WinT;
        let leaf1 = crate::buffer_defs::FrameT { fr_win: win1_ptr, ..Default::default() };
        let leaf2 = crate::buffer_defs::FrameT { fr_win: win2_ptr, ..Default::default() };
        let mut tab2 = crate::buffer_defs::TabpageT {
            tp_topframe: &leaf2 as *const _ as *mut crate::buffer_defs::FrameT,
            tp_ch_used: 0,
            ..Default::default()
        };
        let tab2_ptr = &mut tab2 as *mut crate::buffer_defs::TabpageT;
        let mut tab1 = crate::buffer_defs::TabpageT {
            tp_topframe: &leaf1 as *const _ as *mut crate::buffer_defs::FrameT,
            tp_next: tab2_ptr,
            tp_ch_used: 0,
            ..Default::default()
        };
        let tab1_ptr = &mut tab1 as *mut crate::buffer_defs::TabpageT;

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_firstwin = globals.firstwin;
        let prev_curtab = globals.curtab;
        let prev_first_tabpage = globals.first_tabpage;
        globals.firstwin = win1_ptr;
        globals.curtab = std::ptr::null_mut();
        globals.first_tabpage = tab1_ptr;
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        opts.p_stal = 0;
        opts.p_ls = 0;

        // max(8, 2) + 0 + 0 = 8.
        assert_eq!(unsafe { min_rows_for_all_tabpages() }, 8);

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        globals.firstwin = prev_firstwin;
        globals.curtab = prev_curtab;
        globals.first_tabpage = prev_first_tabpage;
    }

    // ---- did_set_winminheight / did_set_winminwidth ----

    /// RAII guard temporarily setting `OPTION_VARS.p_ch`, restoring
    /// the previous value on drop. Caller must hold
    /// `global_state_test_lock()` for the whole lifetime.
    struct PChGuard {
        prev: crate::types_defs::OptInt,
    }
    impl PChGuard {
        fn set(ch: i64) -> Self {
            let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            let guard = PChGuard { prev: opts.p_ch };
            opts.p_ch = ch;
            guard
        }
    }
    impl Drop for PChGuard {
        fn drop(&mut self) {
            unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ch = self.prev;
        }
    }

    /// Sets up `GLOBALS.firstwin`/`curtab`/`first_tabpage`/`curwin`
    /// for a single-tabpage, single-leaf-frame session (so
    /// `min_rows_for_all_tabpages`/`frame_minwidth` compute a real,
    /// non-`MIN_LINES`-fallback value), restoring the previous
    /// `GLOBALS` fields on drop. Caller must hold
    /// `global_state_test_lock()` for the whole lifetime; `tp_ptr`/
    /// `win_ptr` must outlive the guard.
    struct SingleLeafSessionGuard {
        prev_firstwin: *mut WinT,
        prev_curtab: *mut crate::buffer_defs::TabpageT,
        prev_first_tabpage: *mut crate::buffer_defs::TabpageT,
        prev_curwin: *mut WinT,
        prev_topframe: *mut crate::buffer_defs::FrameT,
    }
    impl SingleLeafSessionGuard {
        fn set(
            win_ptr: *mut WinT,
            tp_ptr: *mut crate::buffer_defs::TabpageT,
            topframe: *mut crate::buffer_defs::FrameT,
        ) -> Self {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            let guard = SingleLeafSessionGuard {
                prev_firstwin: globals.firstwin,
                prev_curtab: globals.curtab,
                prev_first_tabpage: globals.first_tabpage,
                prev_curwin: globals.curwin,
                prev_topframe: globals.topframe,
            };
            globals.firstwin = win_ptr;
            globals.curtab = std::ptr::null_mut(); // tp is NOT curtab
            globals.first_tabpage = tp_ptr;
            globals.curwin = std::ptr::null_mut(); // win_ptr is NOT curwin
            globals.topframe = topframe;
            let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            opts.p_stal = 0; // tabline_height == 0
            opts.p_ls = 0; // global_stl_height == 0
            guard
        }
    }
    impl Drop for SingleLeafSessionGuard {
        fn drop(&mut self) {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            globals.firstwin = self.prev_firstwin;
            globals.curtab = self.prev_curtab;
            globals.first_tabpage = self.prev_first_tabpage;
            globals.curwin = self.prev_curwin;
            globals.topframe = self.prev_topframe;
        }
    }

    #[test]
    fn did_set_winminheight_noop_when_p_wmh_already_non_positive() {
        let _lock = crate::globals::global_state_test_lock();
        let _opts = MinSizeOptsGuard::set(10, 0, 20, 5); // p_wmh = 0
        // The loop's own `while (p_wmh > 0)` never enters - nothing
        // else needs to be set up at all (GLOBALS/OPTION_VARS.p_ch
        // are never even read).
        let mut args = crate::option_defs::OptsetT::default();
        assert_eq!(unsafe { did_set_winminheight(&mut args) }, None);
        assert_eq!(unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_wmh, 0);
    }

    #[test]
    fn did_set_winminheight_no_reduction_when_room_already_fits() {
        let _lock = crate::globals::global_state_test_lock();
        let _opts = MinSizeOptsGuard::set(10, 5, 20, 5); // p_wmh = 5
        let _pch = PChGuard::set(0);
        let mut win =
            WinT { w_winbar_height: 0, w_hsep_height: 0, w_status_height: 0, ..Default::default() };
        let win_ptr = &mut win as *mut WinT;
        let leaf = crate::buffer_defs::FrameT { fr_win: win_ptr, ..Default::default() };
        let leaf_ptr = &leaf as *const _ as *mut crate::buffer_defs::FrameT;
        let mut tp =
            crate::buffer_defs::TabpageT { tp_topframe: leaf_ptr, tp_ch_used: 0, ..Default::default() };
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _session = SingleLeafSessionGuard::set(win_ptr, tp_ptr, leaf_ptr);

        let prev_rows = unsafe { crate::globals::GLOBALS.get_mut() }.Rows;
        unsafe { crate::globals::GLOBALS.get_mut() }.Rows = 24;

        // needed = min_rows_for_all_tabpages() = p_wmh(5) + extras(0) = 5;
        // room = Rows(24) - p_ch(0) = 24 >= 5, so the loop breaks on
        // its very first check - p_wmh stays untouched.
        let mut args = crate::option_defs::OptsetT::default();
        assert_eq!(unsafe { did_set_winminheight(&mut args) }, None);
        assert_eq!(unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_wmh, 5);

        unsafe { crate::globals::GLOBALS.get_mut() }.Rows = prev_rows;
    }

    #[test]
    fn did_set_winminheight_reduces_p_wmh_until_room_fits() {
        let _lock = crate::globals::global_state_test_lock();
        let _opts = MinSizeOptsGuard::set(10, 5, 20, 5); // p_wmh = 5
        let _pch = PChGuard::set(0);
        let mut win =
            WinT { w_winbar_height: 0, w_hsep_height: 0, w_status_height: 0, ..Default::default() };
        let win_ptr = &mut win as *mut WinT;
        let leaf = crate::buffer_defs::FrameT { fr_win: win_ptr, ..Default::default() };
        let leaf_ptr = &leaf as *const _ as *mut crate::buffer_defs::FrameT;
        let mut tp =
            crate::buffer_defs::TabpageT { tp_topframe: leaf_ptr, tp_ch_used: 0, ..Default::default() };
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _session = SingleLeafSessionGuard::set(win_ptr, tp_ptr, leaf_ptr);

        let prev_rows = unsafe { crate::globals::GLOBALS.get_mut() }.Rows;
        unsafe { crate::globals::GLOBALS.get_mut() }.Rows = 3;

        // room = Rows(3) - p_ch(0) = 3.
        // Iteration 1: needed = p_wmh(5) => 5 > 3, p_wmh -= 1 => 4.
        // Iteration 2: needed = p_wmh(4) => 4 > 3, p_wmh -= 1 => 3.
        // Iteration 3: needed = p_wmh(3) => 3 <= 3, break. Final: 3.
        let mut args = crate::option_defs::OptsetT::default();
        assert_eq!(unsafe { did_set_winminheight(&mut args) }, None);
        assert_eq!(unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_wmh, 3);

        unsafe { crate::globals::GLOBALS.get_mut() }.Rows = prev_rows;
    }

    #[test]
    fn did_set_winminwidth_noop_when_p_wmw_already_non_positive() {
        let _lock = crate::globals::global_state_test_lock();
        let _opts = MinSizeOptsGuard::set(10, 2, 20, 0); // p_wmw = 0
        let mut args = crate::option_defs::OptsetT::default();
        assert_eq!(unsafe { did_set_winminwidth(&mut args) }, None);
        assert_eq!(unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_wmw, 0);
    }

    #[test]
    fn did_set_winminwidth_reduces_p_wmw_until_room_fits() {
        let _lock = crate::globals::global_state_test_lock();
        let _opts = MinSizeOptsGuard::set(10, 2, 20, 6); // p_wmw = 6
        let mut win =
            WinT { w_winbar_height: 0, w_hsep_height: 0, w_status_height: 0, w_vsep_width: 0, ..Default::default() };
        let win_ptr = &mut win as *mut WinT;
        let leaf = crate::buffer_defs::FrameT { fr_win: win_ptr, ..Default::default() };
        let leaf_ptr = &leaf as *const _ as *mut crate::buffer_defs::FrameT;

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_topframe = globals.topframe;
        let prev_curwin = globals.curwin;
        let prev_columns = globals.Columns;
        globals.topframe = leaf_ptr;
        globals.curwin = std::ptr::null_mut(); // win_ptr is NOT curwin
        globals.Columns = 4;

        // room = Columns(4).
        // Iteration 1: needed = frame_minwidth(leaf, null) = p_wmw(6) => 6 > 4, p_wmw -= 1 => 5.
        // Iteration 2: needed = p_wmw(5) => 5 > 4, p_wmw -= 1 => 4.
        // Iteration 3: needed = p_wmw(4) => 4 <= 4, break. Final: 4.
        let mut args = crate::option_defs::OptsetT::default();
        assert_eq!(unsafe { did_set_winminwidth(&mut args) }, None);
        assert_eq!(unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_wmw, 4);

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        globals.topframe = prev_topframe;
        globals.curwin = prev_curwin;
        globals.Columns = prev_columns;
    }

    // ---- frame_add_statusline ----

    #[test]
    fn frame_add_statusline_leaf_sets_the_window_status_height() {
        let mut win = WinT { w_status_height: 0, ..Default::default() };
        let win_ptr = &mut win as *mut WinT;
        let mut leaf = crate::buffer_defs::FrameT { fr_win: win_ptr, ..Default::default() };
        let leaf_ptr = &mut leaf as *mut crate::buffer_defs::FrameT;
        unsafe { frame_add_statusline(leaf_ptr) };
        assert_eq!(unsafe { &*win_ptr }.w_status_height, STATUS_HEIGHT);
    }

    #[test]
    fn frame_add_statusline_row_sets_every_child() {
        let mut win1 = WinT { handle: 1, w_status_height: 0, ..Default::default() };
        let mut win2 = WinT { handle: 2, w_status_height: 0, ..Default::default() };
        let win1_ptr = &mut win1 as *mut WinT;
        let win2_ptr = &mut win2 as *mut WinT;
        let mut leaf2 = crate::buffer_defs::FrameT { fr_win: win2_ptr, ..Default::default() };
        let leaf2_ptr = &mut leaf2 as *mut crate::buffer_defs::FrameT;
        let mut leaf1 =
            crate::buffer_defs::FrameT { fr_win: win1_ptr, fr_next: leaf2_ptr, ..Default::default() };
        let mut row = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_ROW,
            fr_child: &mut leaf1 as *mut crate::buffer_defs::FrameT,
            ..Default::default()
        };
        let row_ptr = &mut row as *mut crate::buffer_defs::FrameT;
        unsafe { frame_add_statusline(row_ptr) };
        assert_eq!(unsafe { &*win1_ptr }.w_status_height, STATUS_HEIGHT);
        assert_eq!(unsafe { &*win2_ptr }.w_status_height, STATUS_HEIGHT);
    }

    #[test]
    fn frame_add_statusline_col_only_sets_the_last_child() {
        let mut win1 = WinT { handle: 1, w_status_height: 0, ..Default::default() };
        let mut win2 = WinT { handle: 2, w_status_height: 0, ..Default::default() };
        let win1_ptr = &mut win1 as *mut WinT;
        let win2_ptr = &mut win2 as *mut WinT;
        let mut leaf2 = crate::buffer_defs::FrameT { fr_win: win2_ptr, ..Default::default() };
        let leaf2_ptr = &mut leaf2 as *mut crate::buffer_defs::FrameT;
        let mut leaf1 =
            crate::buffer_defs::FrameT { fr_win: win1_ptr, fr_next: leaf2_ptr, ..Default::default() };
        let mut col = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_COL,
            fr_child: &mut leaf1 as *mut crate::buffer_defs::FrameT,
            ..Default::default()
        };
        let col_ptr = &mut col as *mut crate::buffer_defs::FrameT;
        unsafe { frame_add_statusline(col_ptr) };
        // Only the LAST frame in the column gets a status line - the
        // first is left untouched (matching the original's own
        // "Only need to handle the last frame in the column" comment).
        assert_eq!(unsafe { &*win1_ptr }.w_status_height, 0);
        assert_eq!(unsafe { &*win2_ptr }.w_status_height, STATUS_HEIGHT);
    }

    // ---- frame_add_hsep ----

    #[test]
    fn frame_add_hsep_leaf_sets_the_window_hsep_height() {
        let mut win = WinT { w_hsep_height: 0, ..Default::default() };
        let win_ptr = &mut win as *mut WinT;
        let mut leaf = crate::buffer_defs::FrameT { fr_win: win_ptr, ..Default::default() };
        let leaf_ptr = &mut leaf as *mut crate::buffer_defs::FrameT;
        unsafe { frame_add_hsep(leaf_ptr) };
        assert_eq!(unsafe { &*win_ptr }.w_hsep_height, 1);
    }

    #[test]
    fn frame_add_hsep_row_sets_every_child() {
        let mut win1 = WinT { handle: 1, w_hsep_height: 0, ..Default::default() };
        let mut win2 = WinT { handle: 2, w_hsep_height: 0, ..Default::default() };
        let win1_ptr = &mut win1 as *mut WinT;
        let win2_ptr = &mut win2 as *mut WinT;
        let mut leaf2 = crate::buffer_defs::FrameT { fr_win: win2_ptr, ..Default::default() };
        let leaf2_ptr = &mut leaf2 as *mut crate::buffer_defs::FrameT;
        let mut leaf1 =
            crate::buffer_defs::FrameT { fr_win: win1_ptr, fr_next: leaf2_ptr, ..Default::default() };
        let mut row = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_ROW,
            fr_child: &mut leaf1 as *mut crate::buffer_defs::FrameT,
            ..Default::default()
        };
        let row_ptr = &mut row as *mut crate::buffer_defs::FrameT;
        unsafe { frame_add_hsep(row_ptr) };
        assert_eq!(unsafe { &*win1_ptr }.w_hsep_height, 1);
        assert_eq!(unsafe { &*win2_ptr }.w_hsep_height, 1);
    }

    #[test]
    fn frame_add_hsep_col_only_sets_the_last_child() {
        let mut win1 = WinT { handle: 1, w_hsep_height: 0, ..Default::default() };
        let mut win2 = WinT { handle: 2, w_hsep_height: 0, ..Default::default() };
        let win1_ptr = &mut win1 as *mut WinT;
        let win2_ptr = &mut win2 as *mut WinT;
        let mut leaf2 = crate::buffer_defs::FrameT { fr_win: win2_ptr, ..Default::default() };
        let leaf2_ptr = &mut leaf2 as *mut crate::buffer_defs::FrameT;
        let mut leaf1 =
            crate::buffer_defs::FrameT { fr_win: win1_ptr, fr_next: leaf2_ptr, ..Default::default() };
        let mut col = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_COL,
            fr_child: &mut leaf1 as *mut crate::buffer_defs::FrameT,
            ..Default::default()
        };
        let col_ptr = &mut col as *mut crate::buffer_defs::FrameT;
        unsafe { frame_add_hsep(col_ptr) };
        // Only the LAST frame in the column gets a horizontal
        // separator - the first is left untouched.
        assert_eq!(unsafe { &*win1_ptr }.w_hsep_height, 0);
        assert_eq!(unsafe { &*win2_ptr }.w_hsep_height, 1);
    }

    // ---- find_tabwin ----

    fn unknown_tv() -> TypvalT {
        TypvalT::default()
    }

    fn num_tv(n: crate::eval::typval_defs::VarnumberT) -> TypvalT {
        TypvalT { value: TypvalValue::Number(n), ..Default::default() }
    }

    #[test]
    fn find_tabwin_no_args_returns_curwin() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = focusable_win(1);
        let mut tp = crate::buffer_defs::TabpageT::default();
        let win_ptr = &mut win as *mut WinT;
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = WinnrGlobalsGuard::set(win_ptr, tp_ptr, win_ptr, std::ptr::null_mut(), std::ptr::null_mut());

        assert_eq!(unsafe { find_tabwin(&unknown_tv(), &unknown_tv()) }, win_ptr);
    }

    #[test]
    fn find_tabwin_window_number_only_uses_curtab() {
        let _lock = crate::globals::global_state_test_lock();
        let mut second = focusable_win(2);
        let second_ptr = &mut second as *mut WinT;
        let mut first = WinT { w_next: second_ptr, ..focusable_win(1) };
        let first_ptr = &mut first as *mut WinT;
        let mut tp = crate::buffer_defs::TabpageT::default();
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = WinnrGlobalsGuard::set(first_ptr, tp_ptr, first_ptr, std::ptr::null_mut(), std::ptr::null_mut());

        assert_eq!(unsafe { find_tabwin(&num_tv(2), &unknown_tv()) }, second_ptr);
    }

    #[test]
    fn find_tabwin_with_a_negative_tabnr_returns_null() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = focusable_win(1);
        let mut tp = crate::buffer_defs::TabpageT::default();
        let win_ptr = &mut win as *mut WinT;
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = WinnrGlobalsGuard::set(win_ptr, tp_ptr, win_ptr, std::ptr::null_mut(), std::ptr::null_mut());

        assert!(unsafe { find_tabwin(&num_tv(1), &num_tv(-1)) }.is_null());
    }

    #[test]
    fn find_tabwin_tab_zero_means_curtab() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = focusable_win(1);
        let mut tp = crate::buffer_defs::TabpageT::default();
        let win_ptr = &mut win as *mut WinT;
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = WinnrGlobalsGuard::set(win_ptr, tp_ptr, win_ptr, std::ptr::null_mut(), std::ptr::null_mut());

        // Tab 0 -> find_tabpage(0) -> curtab, matching find_tabpage's
        // own already-established "0 means curtab" convention.
        assert_eq!(unsafe { find_tabwin(&num_tv(1), &num_tv(0)) }, win_ptr);
    }

    // ---- unuse_tabpage / use_tabpage ----

    /// Points `GLOBALS.topframe`/`firstwin`/`lastwin`/`curwin`/`curtab`
    /// and `OPTION_VARS.p_ch` at the given values for the guard's
    /// lifetime, restoring all 6 previous values on drop - a dedicated
    /// guard for `unuse_tabpage`/`use_tabpage`'s own specific field
    /// set (distinct from `CurwinListGuard`'s narrower
    /// `firstwin`/`curtab`/`curwin` coverage; `curtab` must be
    /// restored here too since `use_tabpage` itself mutates it).
    struct WindowListStateGuard {
        prev_topframe: *mut crate::buffer_defs::FrameT,
        prev_firstwin: *mut WinT,
        prev_lastwin: *mut WinT,
        prev_curwin: *mut WinT,
        prev_curtab: *mut crate::buffer_defs::TabpageT,
        prev_ch: crate::types_defs::OptInt,
    }
    impl WindowListStateGuard {
        fn set(
            topframe: *mut crate::buffer_defs::FrameT,
            firstwin: *mut WinT,
            lastwin: *mut WinT,
            curwin: *mut WinT,
            ch: crate::types_defs::OptInt,
        ) -> Self {
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            let guard = WindowListStateGuard {
                prev_topframe: g.topframe,
                prev_firstwin: g.firstwin,
                prev_lastwin: g.lastwin,
                prev_curwin: g.curwin,
                prev_curtab: g.curtab,
                prev_ch: opts.p_ch,
            };
            g.topframe = topframe;
            g.firstwin = firstwin;
            g.lastwin = lastwin;
            g.curwin = curwin;
            opts.p_ch = ch;
            guard
        }
    }
    impl Drop for WindowListStateGuard {
        fn drop(&mut self) {
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            g.topframe = self.prev_topframe;
            g.firstwin = self.prev_firstwin;
            g.lastwin = self.prev_lastwin;
            g.curwin = self.prev_curwin;
            g.curtab = self.prev_curtab;
            unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ch = self.prev_ch;
        }
    }

    #[test]
    fn unuse_tabpage_saves_the_current_window_list_state() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = focusable_win(1);
        let win_ptr = &mut win as *mut WinT;
        let mut frame = crate::buffer_defs::FrameT::default();
        let frame_ptr = &mut frame as *mut crate::buffer_defs::FrameT;
        let mut tp = crate::buffer_defs::TabpageT::default();
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = WindowListStateGuard::set(frame_ptr, win_ptr, win_ptr, win_ptr, 5);

        unsafe { unuse_tabpage(tp_ptr) };

        assert_eq!(unsafe { &*tp_ptr }.tp_topframe, frame_ptr);
        assert_eq!(unsafe { &*tp_ptr }.tp_firstwin, win_ptr);
        assert_eq!(unsafe { &*tp_ptr }.tp_lastwin, win_ptr);
        assert_eq!(unsafe { &*tp_ptr }.tp_curwin, win_ptr);
        assert_eq!(unsafe { &*tp_ptr }.tp_ch_used, 5);
    }

    #[test]
    fn use_tabpage_restores_the_saved_window_list_state() {
        let _lock = crate::globals::global_state_test_lock();
        let mut other_win = focusable_win(2);
        let other_win_ptr = &mut other_win as *mut WinT;
        let mut other_frame = crate::buffer_defs::FrameT::default();
        let other_frame_ptr = &mut other_frame as *mut crate::buffer_defs::FrameT;
        let _guard = WindowListStateGuard::set(
            other_frame_ptr,
            other_win_ptr,
            other_win_ptr,
            other_win_ptr,
            3,
        );

        let mut win = focusable_win(1);
        let win_ptr = &mut win as *mut WinT;
        let mut frame = crate::buffer_defs::FrameT::default();
        let frame_ptr = &mut frame as *mut crate::buffer_defs::FrameT;
        let mut tp = crate::buffer_defs::TabpageT {
            tp_topframe: frame_ptr,
            tp_firstwin: win_ptr,
            tp_lastwin: win_ptr,
            tp_curwin: win_ptr,
            tp_ch_used: 7,
            ..Default::default()
        };
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;

        unsafe { use_tabpage(tp_ptr) };

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        assert_eq!(g.curtab, tp_ptr);
        assert_eq!(g.topframe, frame_ptr);
        assert_eq!(g.firstwin, win_ptr);
        assert_eq!(g.lastwin, win_ptr);
        assert_eq!(g.curwin, win_ptr);
        assert_eq!(unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ch, 7);
    }

    // ---- one_window / last_window ----

    #[test]
    fn one_window_true_when_win_is_the_only_window_via_explicit_tabpage() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = focusable_win(1);
        let win_ptr = &mut win as *mut WinT;
        let mut tp = crate::buffer_defs::TabpageT { tp_firstwin: win_ptr, ..Default::default() };
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let prev_curtab = unsafe { crate::globals::GLOBALS.get_mut() }.curtab;
        unsafe { crate::globals::GLOBALS.get_mut() }.curtab = std::ptr::null_mut();

        assert!(unsafe { one_window(win_ptr, tp_ptr) });

        unsafe { crate::globals::GLOBALS.get_mut() }.curtab = prev_curtab;
    }

    #[test]
    fn one_window_true_when_next_window_is_floating() {
        let _lock = crate::globals::global_state_test_lock();
        let mut floating = focusable_win(2);
        floating.w_floating = true;
        let floating_ptr = &mut floating as *mut WinT;
        let mut win = WinT { w_next: floating_ptr, ..focusable_win(1) };
        let win_ptr = &mut win as *mut WinT;
        let mut tp = crate::buffer_defs::TabpageT { tp_firstwin: win_ptr, ..Default::default() };
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let prev_curtab = unsafe { crate::globals::GLOBALS.get_mut() }.curtab;
        unsafe { crate::globals::GLOBALS.get_mut() }.curtab = std::ptr::null_mut();

        assert!(unsafe { one_window(win_ptr, tp_ptr) });

        unsafe { crate::globals::GLOBALS.get_mut() }.curtab = prev_curtab;
    }

    #[test]
    fn one_window_false_when_next_window_is_non_floating() {
        let _lock = crate::globals::global_state_test_lock();
        let mut second = focusable_win(2);
        let second_ptr = &mut second as *mut WinT;
        let mut win = WinT { w_next: second_ptr, ..focusable_win(1) };
        let win_ptr = &mut win as *mut WinT;
        let mut tp = crate::buffer_defs::TabpageT { tp_firstwin: win_ptr, ..Default::default() };
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let prev_curtab = unsafe { crate::globals::GLOBALS.get_mut() }.curtab;
        unsafe { crate::globals::GLOBALS.get_mut() }.curtab = std::ptr::null_mut();

        assert!(!unsafe { one_window(win_ptr, tp_ptr) });

        unsafe { crate::globals::GLOBALS.get_mut() }.curtab = prev_curtab;
    }

    #[test]
    fn one_window_false_when_win_is_not_the_first_window() {
        let _lock = crate::globals::global_state_test_lock();
        let mut second = focusable_win(2);
        let second_ptr = &mut second as *mut WinT;
        let mut first = WinT { w_next: second_ptr, ..focusable_win(1) };
        let first_ptr = &mut first as *mut WinT;
        let mut tp = crate::buffer_defs::TabpageT { tp_firstwin: first_ptr, ..Default::default() };
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let prev_curtab = unsafe { crate::globals::GLOBALS.get_mut() }.curtab;
        unsafe { crate::globals::GLOBALS.get_mut() }.curtab = std::ptr::null_mut();

        assert!(!unsafe { one_window(second_ptr, tp_ptr) });

        unsafe { crate::globals::GLOBALS.get_mut() }.curtab = prev_curtab;
    }

    #[test]
    fn one_window_null_tp_uses_globals_firstwin() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = focusable_win(1);
        let win_ptr = &mut win as *mut WinT;
        let prev_firstwin = unsafe { crate::globals::GLOBALS.get_mut() }.firstwin;
        unsafe { crate::globals::GLOBALS.get_mut() }.firstwin = win_ptr;

        assert!(unsafe { one_window(win_ptr, std::ptr::null()) });

        unsafe { crate::globals::GLOBALS.get_mut() }.firstwin = prev_firstwin;
    }

    // ---- can_close_floating_windows ----

    #[test]
    fn can_close_floating_windows_true_when_no_floating_windows() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT::default();
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let mut win = WinT { w_floating: false, ..win_with_buffer(1, buf_ptr) };
        let win_ptr = &mut win as *mut WinT;
        let mut tp = crate::buffer_defs::TabpageT { tp_lastwin: win_ptr, ..Default::default() };
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let prev_curtab = unsafe { crate::globals::GLOBALS.get_mut() }.curtab;
        unsafe { crate::globals::GLOBALS.get_mut() }.curtab = std::ptr::null_mut();

        assert!(unsafe { can_close_floating_windows(tp_ptr) });

        unsafe { crate::globals::GLOBALS.get_mut() }.curtab = prev_curtab;
    }

    #[test]
    fn can_close_floating_windows_true_when_floating_window_buffer_unchanged() {
        let _lock = crate::globals::global_state_test_lock();
        let mut normal_buf = crate::buffer_defs::BufT::default();
        let normal_buf_ptr = &mut normal_buf as *mut crate::buffer_defs::BufT;
        let mut normal_win = WinT { w_floating: false, ..win_with_buffer(1, normal_buf_ptr) };
        let normal_win_ptr = &mut normal_win as *mut WinT;

        let mut float_buf = crate::buffer_defs::BufT { b_changed: 0, ..Default::default() };
        let float_buf_ptr = &mut float_buf as *mut crate::buffer_defs::BufT;
        let mut float_win = WinT {
            w_floating: true,
            w_prev: normal_win_ptr,
            ..win_with_buffer(2, float_buf_ptr)
        };
        let float_win_ptr = &mut float_win as *mut WinT;

        let mut tp = crate::buffer_defs::TabpageT { tp_lastwin: float_win_ptr, ..Default::default() };
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let prev_curtab = unsafe { crate::globals::GLOBALS.get_mut() }.curtab;
        unsafe { crate::globals::GLOBALS.get_mut() }.curtab = std::ptr::null_mut();

        assert!(unsafe { can_close_floating_windows(tp_ptr) });

        unsafe { crate::globals::GLOBALS.get_mut() }.curtab = prev_curtab;
    }

    #[test]
    fn can_close_floating_windows_true_when_changed_buffer_has_other_windows() {
        let _lock = crate::globals::global_state_test_lock();
        let mut normal_buf = crate::buffer_defs::BufT::default();
        let normal_buf_ptr = &mut normal_buf as *mut crate::buffer_defs::BufT;
        let mut normal_win = WinT { w_floating: false, ..win_with_buffer(1, normal_buf_ptr) };
        let normal_win_ptr = &mut normal_win as *mut WinT;

        // b_changed set, but b_nwindows > 1 means need_hide is false
        // regardless (the buffer is still visible in ANOTHER window).
        let mut float_buf =
            crate::buffer_defs::BufT { b_changed: 1, b_nwindows: 2, ..Default::default() };
        let float_buf_ptr = &mut float_buf as *mut crate::buffer_defs::BufT;
        let mut float_win = WinT {
            w_floating: true,
            w_prev: normal_win_ptr,
            ..win_with_buffer(2, float_buf_ptr)
        };
        let float_win_ptr = &mut float_win as *mut WinT;

        let mut tp = crate::buffer_defs::TabpageT { tp_lastwin: float_win_ptr, ..Default::default() };
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let prev_curtab = unsafe { crate::globals::GLOBALS.get_mut() }.curtab;
        unsafe { crate::globals::GLOBALS.get_mut() }.curtab = std::ptr::null_mut();

        assert!(unsafe { can_close_floating_windows(tp_ptr) });

        unsafe { crate::globals::GLOBALS.get_mut() }.curtab = prev_curtab;
    }

    #[test]
    fn can_close_floating_windows_true_when_changed_buffer_can_be_hidden() {
        let _lock = crate::globals::global_state_test_lock();
        let mut normal_buf = crate::buffer_defs::BufT::default();
        let normal_buf_ptr = &mut normal_buf as *mut crate::buffer_defs::BufT;
        let mut normal_win = WinT { w_floating: false, ..win_with_buffer(1, normal_buf_ptr) };
        let normal_win_ptr = &mut normal_win as *mut WinT;

        let mut float_buf = crate::buffer_defs::BufT {
            b_changed: 1,
            b_nwindows: 1,
            b_p_bh: Some(b"hide".to_vec()),
            ..Default::default()
        };
        let float_buf_ptr = &mut float_buf as *mut crate::buffer_defs::BufT;
        let mut float_win = WinT {
            w_floating: true,
            w_prev: normal_win_ptr,
            ..win_with_buffer(2, float_buf_ptr)
        };
        let float_win_ptr = &mut float_win as *mut WinT;

        let mut tp = crate::buffer_defs::TabpageT { tp_lastwin: float_win_ptr, ..Default::default() };
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let prev_curtab = unsafe { crate::globals::GLOBALS.get_mut() }.curtab;
        unsafe { crate::globals::GLOBALS.get_mut() }.curtab = std::ptr::null_mut();

        assert!(unsafe { can_close_floating_windows(tp_ptr) });

        unsafe { crate::globals::GLOBALS.get_mut() }.curtab = prev_curtab;
    }

    #[test]
    fn can_close_floating_windows_false_when_changed_buffer_cannot_be_hidden() {
        let _lock = crate::globals::global_state_test_lock();
        let mut normal_buf = crate::buffer_defs::BufT::default();
        let normal_buf_ptr = &mut normal_buf as *mut crate::buffer_defs::BufT;
        let mut normal_win = WinT { w_floating: false, ..win_with_buffer(1, normal_buf_ptr) };
        let normal_win_ptr = &mut normal_win as *mut WinT;

        // b_p_bh = "unload" -> buf_hide always returns false, regardless
        // of the global 'hidden' option.
        let mut float_buf = crate::buffer_defs::BufT {
            b_changed: 1,
            b_nwindows: 1,
            b_p_bh: Some(b"unload".to_vec()),
            ..Default::default()
        };
        let float_buf_ptr = &mut float_buf as *mut crate::buffer_defs::BufT;
        let mut float_win = WinT {
            w_floating: true,
            w_prev: normal_win_ptr,
            ..win_with_buffer(2, float_buf_ptr)
        };
        let float_win_ptr = &mut float_win as *mut WinT;

        let mut tp = crate::buffer_defs::TabpageT { tp_lastwin: float_win_ptr, ..Default::default() };
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let prev_curtab = unsafe { crate::globals::GLOBALS.get_mut() }.curtab;
        unsafe { crate::globals::GLOBALS.get_mut() }.curtab = std::ptr::null_mut();

        assert!(!unsafe { can_close_floating_windows(tp_ptr) });

        unsafe { crate::globals::GLOBALS.get_mut() }.curtab = prev_curtab;
    }

    #[test]
    fn can_close_floating_windows_null_tp_uses_globals_lastwin() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT::default();
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let mut win = WinT { w_floating: false, ..win_with_buffer(1, buf_ptr) };
        let win_ptr = &mut win as *mut WinT;
        let mut tp = crate::buffer_defs::TabpageT::default();
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let prev_lastwin = unsafe { crate::globals::GLOBALS.get_mut() }.lastwin;
        let prev_curtab = unsafe { crate::globals::GLOBALS.get_mut() }.curtab;
        unsafe { crate::globals::GLOBALS.get_mut() }.lastwin = win_ptr;
        // curtab must be some real, non-null tabpage DIFFERENT from the
        // null `tp` passed below, matching a real running session
        // (curtab is never null) and satisfying the original's own
        // `tp != curtab` debug assertion.
        unsafe { crate::globals::GLOBALS.get_mut() }.curtab = tp_ptr;

        assert!(unsafe { can_close_floating_windows(std::ptr::null()) });

        unsafe { crate::globals::GLOBALS.get_mut() }.lastwin = prev_lastwin;
        unsafe { crate::globals::GLOBALS.get_mut() }.curtab = prev_curtab;
    }

    // ---- leaving_window / entering_window ----

    /// Saves/restores `GLOBALS.restart_edit`/`mode_displayed`/
    /// `clear_cmdline`/`Ins.stop_insert_mode`/`State` for the guard's
    /// lifetime - the fields `leaving_window`/`entering_window` touch.
    struct PromptWindowGlobalsGuard {
        prev_restart_edit: i32,
        prev_mode_displayed: bool,
        prev_clear_cmdline: bool,
        prev_stop_insert_mode: bool,
        prev_state: i32,
    }
    impl PromptWindowGlobalsGuard {
        fn set(restart_edit: i32, mode_displayed: bool, state: i32, stop_insert_mode: bool) -> Self {
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            let guard = PromptWindowGlobalsGuard {
                prev_restart_edit: g.restart_edit,
                prev_mode_displayed: g.mode_displayed,
                prev_clear_cmdline: g.clear_cmdline,
                prev_stop_insert_mode: g.Ins.stop_insert_mode,
                prev_state: g.State,
            };
            g.restart_edit = restart_edit;
            g.mode_displayed = mode_displayed;
            g.clear_cmdline = false;
            g.Ins.stop_insert_mode = stop_insert_mode;
            g.State = state;
            guard
        }
    }
    impl Drop for PromptWindowGlobalsGuard {
        fn drop(&mut self) {
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            g.restart_edit = self.prev_restart_edit;
            g.mode_displayed = self.prev_mode_displayed;
            g.clear_cmdline = self.prev_clear_cmdline;
            g.Ins.stop_insert_mode = self.prev_stop_insert_mode;
            g.State = self.prev_state;
        }
    }

    #[test]
    fn leaving_window_no_op_for_a_non_prompt_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = PromptWindowGlobalsGuard::set(1, true, 0, false);
        let mut buf = crate::buffer_defs::BufT::default();
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let mut win = win_with_buffer(1, buf_ptr);
        let win_ptr = &mut win as *mut WinT;

        unsafe { leaving_window(win_ptr) };

        // Nothing touched: restart_edit stays 1, b_prompt_insert stays 0.
        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.restart_edit, 1);
        assert_eq!(unsafe { &*buf_ptr }.b_prompt_insert, 0);
    }

    #[test]
    fn leaving_window_saves_restart_edit_and_clears_it() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = PromptWindowGlobalsGuard::set(i32::from(b'i'), true, 0, false);
        let mut buf =
            crate::buffer_defs::BufT { b_p_bt: Some(b"prompt".to_vec()), ..Default::default() };
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let mut win = win_with_buffer(1, buf_ptr);
        let win_ptr = &mut win as *mut WinT;

        unsafe { leaving_window(win_ptr) };

        assert_eq!(unsafe { &*buf_ptr }.b_prompt_insert, i32::from(b'i'));
        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.restart_edit, 0);
        assert!(unsafe { crate::globals::GLOBALS.get_mut() }.clear_cmdline);
    }

    #[test]
    fn leaving_window_leaves_clear_cmdline_untouched_when_restart_edit_is_zero() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = PromptWindowGlobalsGuard::set(0, true, 0, false);
        let mut buf =
            crate::buffer_defs::BufT { b_p_bt: Some(b"prompt".to_vec()), ..Default::default() };
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let mut win = win_with_buffer(1, buf_ptr);
        let win_ptr = &mut win as *mut WinT;

        unsafe { leaving_window(win_ptr) };

        assert!(!unsafe { crate::globals::GLOBALS.get_mut() }.clear_cmdline);
    }

    #[test]
    fn leaving_window_stops_insert_mode_and_defaults_prompt_insert_to_a() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard =
            PromptWindowGlobalsGuard::set(0, false, crate::state_defs::mode::INSERT as i32, false);
        let mut buf =
            crate::buffer_defs::BufT { b_p_bt: Some(b"prompt".to_vec()), ..Default::default() };
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let mut win = win_with_buffer(1, buf_ptr);
        let win_ptr = &mut win as *mut WinT;

        unsafe { leaving_window(win_ptr) };

        assert!(unsafe { crate::globals::GLOBALS.get_mut() }.Ins.stop_insert_mode);
        // b_prompt_insert was 0 (from restart_edit == 0), so the
        // insert-mode-interrupted branch defaults it to 'A'.
        assert_eq!(unsafe { &*buf_ptr }.b_prompt_insert, i32::from(b'A'));
    }

    #[test]
    fn leaving_window_does_not_stop_insert_mode_when_already_stopped() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard =
            PromptWindowGlobalsGuard::set(0, false, crate::state_defs::mode::INSERT as i32, true);
        let mut buf =
            crate::buffer_defs::BufT { b_p_bt: Some(b"prompt".to_vec()), ..Default::default() };
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let mut win = win_with_buffer(1, buf_ptr);
        let win_ptr = &mut win as *mut WinT;

        unsafe { leaving_window(win_ptr) };

        // Already true beforehand; b_prompt_insert stays 0 (the
        // "defaults to 'A'" branch is only reached when
        // stop_insert_mode is newly set here).
        assert_eq!(unsafe { &*buf_ptr }.b_prompt_insert, 0);
    }

    #[test]
    fn entering_window_no_op_for_a_non_prompt_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = PromptWindowGlobalsGuard::set(0, false, 0, true);
        let mut buf = crate::buffer_defs::BufT::default();
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let mut win = win_with_buffer(1, buf_ptr);
        let win_ptr = &mut win as *mut WinT;

        unsafe { entering_window(win_ptr) };

        assert!(unsafe { crate::globals::GLOBALS.get_mut() }.Ins.stop_insert_mode);
    }

    #[test]
    fn entering_window_unsets_stop_insert_mode_when_b_prompt_insert_is_set() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard =
            PromptWindowGlobalsGuard::set(0, false, crate::state_defs::mode::INSERT as i32, true);
        let mut buf = crate::buffer_defs::BufT {
            b_p_bt: Some(b"prompt".to_vec()),
            b_prompt_insert: i32::from(b'A'),
            ..Default::default()
        };
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let mut win = win_with_buffer(1, buf_ptr);
        let win_ptr = &mut win as *mut WinT;

        unsafe { entering_window(win_ptr) };

        assert!(!unsafe { crate::globals::GLOBALS.get_mut() }.Ins.stop_insert_mode);
    }

    #[test]
    fn entering_window_restarts_insert_mode_when_not_already_in_insert_mode() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = PromptWindowGlobalsGuard::set(0, false, 0, false);
        let mut buf = crate::buffer_defs::BufT {
            b_p_bt: Some(b"prompt".to_vec()),
            b_prompt_insert: i32::from(b'A'),
            ..Default::default()
        };
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let mut win = win_with_buffer(1, buf_ptr);
        let win_ptr = &mut win as *mut WinT;

        unsafe { entering_window(win_ptr) };

        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.restart_edit, i32::from(b'A'));
    }

    #[test]
    fn entering_window_leaves_restart_edit_untouched_when_already_in_insert_mode() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard =
            PromptWindowGlobalsGuard::set(7, false, crate::state_defs::mode::INSERT as i32, false);
        let mut buf = crate::buffer_defs::BufT {
            b_p_bt: Some(b"prompt".to_vec()),
            b_prompt_insert: i32::from(b'A'),
            ..Default::default()
        };
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let mut win = win_with_buffer(1, buf_ptr);
        let win_ptr = &mut win as *mut WinT;

        unsafe { entering_window(win_ptr) };

        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.restart_edit, 7);
    }

    // ---- trigger_winnewpre / do_autocmd_winclosed / trigger_tabclosedpre ----

    #[test]
    fn trigger_winnewpre_is_a_real_no_op_since_nothing_can_register_the_autocmd() {
        let _lock = crate::globals::global_state_test_lock();
        let prev_split = unsafe { *SPLIT_DISALLOWED.get_mut() };
        let prev_close = unsafe { *CLOSE_DISALLOWED.get_mut() };

        trigger_winnewpre(); // must not panic

        // window_layout_lock/unlock leave both counters exactly as
        // they started (one increment, one matching decrement).
        assert_eq!(unsafe { *SPLIT_DISALLOWED.get_mut() }, prev_split);
        assert_eq!(unsafe { *CLOSE_DISALLOWED.get_mut() }, prev_close);
    }

    #[test]
    fn do_autocmd_winclosed_returns_early_when_recursive() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { *DO_AUTOCMD_WINCLOSED_RECURSIVE.get_mut() = true };
        do_autocmd_winclosed(std::ptr::null()); // must not panic
        unsafe { *DO_AUTOCMD_WINCLOSED_RECURSIVE.get_mut() = false };
    }

    #[test]
    fn do_autocmd_winclosed_returns_early_when_has_event_is_false() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { *DO_AUTOCMD_WINCLOSED_RECURSIVE.get_mut() = false };
        // WinClosed's own registry is always empty today - has_event
        // is always false, so this must not panic.
        do_autocmd_winclosed(std::ptr::null());
    }

    #[test]
    fn trigger_tabclosedpre_returns_early_when_recursive() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { *TRIGGER_TABCLOSEDPRE_RECURSIVE.get_mut() = true };
        trigger_tabclosedpre(std::ptr::null()); // must not panic
        unsafe { *TRIGGER_TABCLOSEDPRE_RECURSIVE.get_mut() = false };
    }

    #[test]
    fn trigger_tabclosedpre_returns_early_when_has_event_is_false() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { *TRIGGER_TABCLOSEDPRE_RECURSIVE.get_mut() = false };
        // TabClosedPre's own registry is always empty today -
        // has_event is always false, so this must not panic.
        trigger_tabclosedpre(std::ptr::null());
    }

    // ---- global_winbar_height / get_maximum_wincount ----

    #[test]
    fn global_winbar_height_zero_when_winbar_unset() {
        let _lock = crate::globals::global_state_test_lock();
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let prev = opts.p_wbr.take();
        assert_eq!(unsafe { global_winbar_height() }, 0);
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_wbr = prev;
    }

    #[test]
    fn global_winbar_height_zero_when_winbar_is_empty_string() {
        let _lock = crate::globals::global_state_test_lock();
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let prev = opts.p_wbr.replace(Vec::new());
        assert_eq!(unsafe { global_winbar_height() }, 0);
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_wbr = prev;
    }

    #[test]
    fn global_winbar_height_one_when_winbar_is_set() {
        let _lock = crate::globals::global_state_test_lock();
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let prev = opts.p_wbr.replace(b"%f".to_vec());
        assert_eq!(unsafe { global_winbar_height() }, 1);
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_wbr = prev;
    }

    /// Saves/restores `OPTION_VARS.p_wmh`/`p_wbr` for the guard's
    /// lifetime.
    struct WinbarWmhGuard {
        prev_wmh: crate::types_defs::OptInt,
        prev_wbr: Option<Vec<u8>>,
    }
    impl WinbarWmhGuard {
        fn set(wmh: i64, wbr: Option<&[u8]>) -> Self {
            let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            let guard =
                WinbarWmhGuard { prev_wmh: opts.p_wmh, prev_wbr: opts.p_wbr.take() };
            opts.p_wmh = wmh;
            opts.p_wbr = wbr.map(<[u8]>::to_vec);
            guard
        }
    }
    impl Drop for WinbarWmhGuard {
        fn drop(&mut self) {
            let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            opts.p_wmh = self.prev_wmh;
            opts.p_wbr = self.prev_wbr.take();
        }
    }

    #[test]
    fn get_maximum_wincount_non_col_frame_uses_frame2win_directly() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = WinbarWmhGuard::set(1, None);
        let mut win = WinT { w_winbar_height: 0, ..focusable_win(1) };
        let win_ptr = &mut win as *mut WinT;
        let leaf = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_LEAF,
            fr_win: win_ptr,
            ..Default::default()
        };
        // height=10, p_wmh=1, STATUS_HEIGHT=1, winbar_height=0 -> 10/2 = 5.
        assert_eq!(unsafe { get_maximum_wincount(&leaf, 10) }, 5);
    }

    #[test]
    fn get_maximum_wincount_col_frame_with_global_winbar() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = WinbarWmhGuard::set(1, Some(b"%f"));
        let col = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_COL,
            ..Default::default()
        };
        // height=12, p_wmh=1, STATUS_HEIGHT=1, +1 for global winbar -> 12/3 = 4.
        assert_eq!(unsafe { get_maximum_wincount(&col, 12) }, 4);
    }

    #[test]
    fn get_maximum_wincount_col_frame_without_global_winbar_sums_children() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = WinbarWmhGuard::set(1, None);
        let mut win1 = WinT { w_winbar_height: 0, ..focusable_win(1) };
        let win1_ptr = &mut win1 as *mut WinT;
        let mut win2 = WinT { w_winbar_height: 0, ..focusable_win(2) };
        let win2_ptr = &mut win2 as *mut WinT;

        let mut leaf2 = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_LEAF,
            fr_win: win2_ptr,
            ..Default::default()
        };
        let leaf2_ptr = &mut leaf2 as *mut crate::buffer_defs::FrameT;
        let leaf1 = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_LEAF,
            fr_win: win1_ptr,
            fr_next: leaf2_ptr,
            ..Default::default()
        };
        let col = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_COL,
            fr_child: &leaf1 as *const _ as *mut crate::buffer_defs::FrameT,
            ..Default::default()
        };
        // Each child needs 2 rows (p_wmh=1 + STATUS_HEIGHT=1); with
        // height=10, both children fit (uses 4), leaving 6 more ->
        // 6/2=3 additional -> 2 (real children) + 3 = 5.
        assert_eq!(unsafe { get_maximum_wincount(&col, 10) }, 5);
    }

    // ---- only_one_window ----

    /// Saves/restores `GLOBALS.first_tabpage`/`firstwin`/`curbuf`/
    /// `curwin` for the guard's lifetime.
    struct OnlyOneWindowGuard {
        prev_first_tabpage: *mut crate::buffer_defs::TabpageT,
        prev_firstwin: *mut WinT,
        prev_curbuf: *mut crate::buffer_defs::BufT,
        prev_curwin: *mut WinT,
    }
    impl OnlyOneWindowGuard {
        fn set(
            first_tabpage: *mut crate::buffer_defs::TabpageT,
            firstwin: *mut WinT,
            curbuf: *mut crate::buffer_defs::BufT,
            curwin: *mut WinT,
        ) -> Self {
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            let guard = OnlyOneWindowGuard {
                prev_first_tabpage: g.first_tabpage,
                prev_firstwin: g.firstwin,
                prev_curbuf: g.curbuf,
                prev_curwin: g.curwin,
            };
            g.first_tabpage = first_tabpage;
            g.firstwin = firstwin;
            g.curbuf = curbuf;
            g.curwin = curwin;
            guard
        }
    }
    impl Drop for OnlyOneWindowGuard {
        fn drop(&mut self) {
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            g.first_tabpage = self.prev_first_tabpage;
            g.firstwin = self.prev_firstwin;
            g.curbuf = self.prev_curbuf;
            g.curwin = self.prev_curwin;
        }
    }

    #[test]
    fn only_one_window_false_when_another_tabpage_exists() {
        let _lock = crate::globals::global_state_test_lock();
        let mut second_tp = crate::buffer_defs::TabpageT::default();
        let second_tp_ptr = &mut second_tp as *mut crate::buffer_defs::TabpageT;
        let mut tp = crate::buffer_defs::TabpageT { tp_next: second_tp_ptr, ..Default::default() };
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = OnlyOneWindowGuard::set(tp_ptr, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut());

        assert!(!unsafe { only_one_window() });
    }

    #[test]
    fn only_one_window_true_for_a_single_normal_window() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT::default();
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let mut win = win_with_buffer(1, buf_ptr);
        let win_ptr = &mut win as *mut WinT;
        let mut tp = crate::buffer_defs::TabpageT { tp_next: std::ptr::null_mut(), ..Default::default() };
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = OnlyOneWindowGuard::set(tp_ptr, win_ptr, buf_ptr, win_ptr);

        assert!(unsafe { only_one_window() });
    }

    #[test]
    fn only_one_window_false_for_two_normal_windows() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf1 = crate::buffer_defs::BufT::default();
        let buf1_ptr = &mut buf1 as *mut crate::buffer_defs::BufT;
        let mut buf2 = crate::buffer_defs::BufT::default();
        let buf2_ptr = &mut buf2 as *mut crate::buffer_defs::BufT;
        let mut win2 = win_with_buffer(2, buf2_ptr);
        let win2_ptr = &mut win2 as *mut WinT;
        let mut win1 = WinT { w_next: win2_ptr, ..win_with_buffer(1, buf1_ptr) };
        let win1_ptr = &mut win1 as *mut WinT;
        let mut tp = crate::buffer_defs::TabpageT { tp_next: std::ptr::null_mut(), ..Default::default() };
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = OnlyOneWindowGuard::set(tp_ptr, win1_ptr, buf1_ptr, win1_ptr);

        assert!(!unsafe { only_one_window() });
    }

    #[test]
    fn only_one_window_help_window_not_curwin_does_not_count() {
        let _lock = crate::globals::global_state_test_lock();
        let mut curbuf = crate::buffer_defs::BufT::default();
        let curbuf_ptr = &mut curbuf as *mut crate::buffer_defs::BufT;
        let mut help_buf = crate::buffer_defs::BufT { b_help: true, ..Default::default() };
        let help_buf_ptr = &mut help_buf as *mut crate::buffer_defs::BufT;
        let mut help_win = win_with_buffer(2, help_buf_ptr);
        let help_win_ptr = &mut help_win as *mut WinT;
        let mut cur_win = WinT { w_next: help_win_ptr, ..win_with_buffer(1, curbuf_ptr) };
        let cur_win_ptr = &mut cur_win as *mut WinT;
        let mut tp = crate::buffer_defs::TabpageT { tp_next: std::ptr::null_mut(), ..Default::default() };
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = OnlyOneWindowGuard::set(tp_ptr, cur_win_ptr, curbuf_ptr, cur_win_ptr);

        // help_win doesn't count (help buffer, curbuf isn't help, not
        // curwin) - only cur_win counts -> count == 1 -> true.
        assert!(unsafe { only_one_window() });
    }

    #[test]
    fn only_one_window_help_window_that_is_curwin_counts() {
        let _lock = crate::globals::global_state_test_lock();
        let mut help_buf = crate::buffer_defs::BufT { b_help: true, ..Default::default() };
        let help_buf_ptr = &mut help_buf as *mut crate::buffer_defs::BufT;
        let mut normal_buf = crate::buffer_defs::BufT::default();
        let normal_buf_ptr = &mut normal_buf as *mut crate::buffer_defs::BufT;
        let mut normal_win = win_with_buffer(2, normal_buf_ptr);
        let normal_win_ptr = &mut normal_win as *mut WinT;
        let mut help_win = WinT { w_next: normal_win_ptr, ..win_with_buffer(1, help_buf_ptr) };
        let help_win_ptr = &mut help_win as *mut WinT;
        let mut tp = crate::buffer_defs::TabpageT { tp_next: std::ptr::null_mut(), ..Default::default() };
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        // help_win IS curwin (counts, since `wp == curwin`), normal_win
        // is a real, non-special buffer (also counts) -> count == 2 ->
        // false.
        let _guard = OnlyOneWindowGuard::set(tp_ptr, help_win_ptr, help_buf_ptr, help_win_ptr);

        assert!(!unsafe { only_one_window() });
    }

    #[test]
    fn only_one_window_ctx_window_never_counts() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf1 = crate::buffer_defs::BufT::default();
        let buf1_ptr = &mut buf1 as *mut crate::buffer_defs::BufT;
        let mut buf2 = crate::buffer_defs::BufT::default();
        let buf2_ptr = &mut buf2 as *mut crate::buffer_defs::BufT;
        let mut ctx_win = win_with_buffer(2, buf2_ptr);
        let ctx_win_ptr = &mut ctx_win as *mut WinT;
        let mut real_win = WinT { w_next: ctx_win_ptr, ..win_with_buffer(1, buf1_ptr) };
        let real_win_ptr = &mut real_win as *mut WinT;
        let mut tp = crate::buffer_defs::TabpageT { tp_next: std::ptr::null_mut(), ..Default::default() };
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = OnlyOneWindowGuard::set(tp_ptr, real_win_ptr, buf1_ptr, real_win_ptr);

        unsafe { crate::context::CTX_WIN_VEC.get_mut() }
            .push(crate::context_defs::CtxWin { cw_win: ctx_win_ptr, cw_used: true });

        // ctx_win never counts regardless of its own buffer/curwin
        // status - only real_win counts -> count == 1 -> true.
        assert!(unsafe { only_one_window() });

        unsafe { crate::context::CTX_WIN_VEC.get_mut() }.clear();
    }

    // ---- get_last_winid / win_locked ----

    #[test]
    fn get_last_winid_defaults_to_one_below_lowest_win_id() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { *LAST_WIN_ID.get_mut() };
        unsafe { *LAST_WIN_ID.get_mut() = LOWEST_WIN_ID - 1 };
        assert_eq!(get_last_winid(), LOWEST_WIN_ID - 1);
        unsafe { *LAST_WIN_ID.get_mut() = prev };
    }

    #[test]
    fn get_last_winid_reflects_the_underlying_counter() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { *LAST_WIN_ID.get_mut() };
        unsafe { *LAST_WIN_ID.get_mut() = 1042 };
        assert_eq!(get_last_winid(), 1042);
        unsafe { *LAST_WIN_ID.get_mut() = prev };
    }

    #[test]
    fn win_locked_reads_w_locked_directly() {
        let win_unlocked = WinT { w_locked: 0, ..focusable_win(1) };
        assert_eq!(unsafe { win_locked(&win_unlocked) }, 0);

        let win_locked_win = WinT { w_locked: 1, ..focusable_win(1) };
        assert_eq!(unsafe { win_locked(&win_locked_win) }, 1);
    }

    // ---- merge_win_config / clear_float_config ----

    #[test]
    fn merge_win_config_replaces_dst_entirely() {
        let mut dst = crate::buffer_defs::WinConfig {
            height: 5,
            width: 5,
            style: crate::buffer_defs::WinStyle::Minimal,
            ..Default::default()
        };
        let src = crate::buffer_defs::WinConfig {
            height: 20,
            width: 30,
            style: crate::buffer_defs::WinStyle::Unused,
            ..Default::default()
        };
        merge_win_config(&mut dst, src.clone());
        assert_eq!(dst.height, 20);
        assert_eq!(dst.width, 30);
        assert_eq!(dst.style, crate::buffer_defs::WinStyle::Unused);
    }

    #[test]
    fn clear_float_config_resets_to_default_but_preserves_style_and_cmdline_offset() {
        let mut fconfig = crate::buffer_defs::WinConfig {
            height: 10,
            width: 15,
            style: crate::buffer_defs::WinStyle::Minimal,
            _cmdline_offset: 42,
            external: true,
            ..Default::default()
        };
        clear_float_config(&mut fconfig, true);

        let expected = crate::buffer_defs::WinConfig {
            style: crate::buffer_defs::WinStyle::Minimal,
            _cmdline_offset: 42,
            ..Default::default()
        };
        assert_eq!(fconfig.height, expected.height);
        assert_eq!(fconfig.width, expected.width);
        assert_eq!(fconfig.external, expected.external);
        assert_eq!(fconfig.style, crate::buffer_defs::WinStyle::Minimal);
        assert_eq!(fconfig._cmdline_offset, 42);
    }

    #[test]
    fn clear_float_config_free_fields_false_has_the_same_observable_effect() {
        let mut fconfig = crate::buffer_defs::WinConfig {
            height: 10,
            style: crate::buffer_defs::WinStyle::Minimal,
            _cmdline_offset: 7,
            ..Default::default()
        };
        clear_float_config(&mut fconfig, false);

        assert_eq!(fconfig.height, 0);
        assert_eq!(fconfig.style, crate::buffer_defs::WinStyle::Minimal);
        assert_eq!(fconfig._cmdline_offset, 7);
    }

    // ---- valid_tabpage_win ----

    /// Saves/restores `GLOBALS.first_tabpage`/`curtab`/`firstwin` for
    /// the guard's lifetime - the exact field set `valid_tabpage_win`
    /// needs (distinct from `OnlyOneWindowGuard`, which manages
    /// `curbuf`/`curwin` instead of `curtab`).
    struct ValidTabpageWinGuard {
        prev_first_tabpage: *mut crate::buffer_defs::TabpageT,
        prev_curtab: *mut crate::buffer_defs::TabpageT,
        prev_firstwin: *mut WinT,
    }
    impl ValidTabpageWinGuard {
        fn set(
            first_tabpage: *mut crate::buffer_defs::TabpageT,
            curtab: *mut crate::buffer_defs::TabpageT,
            firstwin: *mut WinT,
        ) -> Self {
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            let guard = ValidTabpageWinGuard {
                prev_first_tabpage: g.first_tabpage,
                prev_curtab: g.curtab,
                prev_firstwin: g.firstwin,
            };
            g.first_tabpage = first_tabpage;
            g.curtab = curtab;
            g.firstwin = firstwin;
            guard
        }
    }
    impl Drop for ValidTabpageWinGuard {
        fn drop(&mut self) {
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            g.first_tabpage = self.prev_first_tabpage;
            g.curtab = self.prev_curtab;
            g.firstwin = self.prev_firstwin;
        }
    }

    #[test]
    fn valid_tabpage_win_true_for_curtab_with_a_valid_window() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = focusable_win(1);
        let win_ptr = &mut win as *mut WinT;
        let mut tp = crate::buffer_defs::TabpageT::default();
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = ValidTabpageWinGuard::set(tp_ptr, tp_ptr, win_ptr);

        assert!(unsafe { valid_tabpage_win(tp_ptr) });
    }

    #[test]
    fn valid_tabpage_win_false_for_curtab_with_no_windows() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = ValidTabpageWinGuard::set(tp_ptr, tp_ptr, std::ptr::null_mut());

        assert!(!unsafe { valid_tabpage_win(tp_ptr) });
    }

    #[test]
    fn valid_tabpage_win_true_for_a_non_curtab_with_its_own_window() {
        let _lock = crate::globals::global_state_test_lock();
        let mut other_win = focusable_win(2);
        let other_win_ptr = &mut other_win as *mut WinT;
        let mut other_tp =
            crate::buffer_defs::TabpageT { tp_firstwin: other_win_ptr, ..Default::default() };
        let other_tp_ptr = &mut other_tp as *mut crate::buffer_defs::TabpageT;

        let mut cur_win = focusable_win(1);
        let cur_win_ptr = &mut cur_win as *mut WinT;
        let mut cur_tp =
            crate::buffer_defs::TabpageT { tp_next: other_tp_ptr, ..Default::default() };
        let cur_tp_ptr = &mut cur_tp as *mut crate::buffer_defs::TabpageT;
        // curtab is cur_tp_ptr, NOT other_tp_ptr - proving other_tp's
        // own tp_firstwin is used, not GLOBALS.firstwin.
        let _guard = ValidTabpageWinGuard::set(cur_tp_ptr, cur_tp_ptr, cur_win_ptr);

        assert!(unsafe { valid_tabpage_win(other_tp_ptr) });
    }

    #[test]
    fn valid_tabpage_win_false_when_tpc_is_not_in_the_tabpage_list() {
        let _lock = crate::globals::global_state_test_lock();
        let mut cur_win = focusable_win(1);
        let cur_win_ptr = &mut cur_win as *mut WinT;
        let mut cur_tp =
            crate::buffer_defs::TabpageT { tp_next: std::ptr::null_mut(), ..Default::default() };
        let cur_tp_ptr = &mut cur_tp as *mut crate::buffer_defs::TabpageT;
        let _guard = ValidTabpageWinGuard::set(cur_tp_ptr, cur_tp_ptr, cur_win_ptr);

        let mut unrelated_tp = crate::buffer_defs::TabpageT::default();
        let unrelated_tp_ptr = &mut unrelated_tp as *mut crate::buffer_defs::TabpageT;

        // shouldn't happen in practice, but must not panic.
        assert!(!unsafe { valid_tabpage_win(unrelated_tp_ptr) });
    }

    #[test]
    fn last_window_true_when_one_window_and_no_other_tabpage() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = focusable_win(1);
        let win_ptr = &mut win as *mut WinT;
        let mut tp =
            crate::buffer_defs::TabpageT { tp_next: std::ptr::null_mut(), ..Default::default() };
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let prev_firstwin = unsafe { crate::globals::GLOBALS.get_mut() }.firstwin;
        let prev_first_tabpage = unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage;
        unsafe { crate::globals::GLOBALS.get_mut() }.firstwin = win_ptr;
        unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage = tp_ptr;

        assert!(unsafe { last_window(win_ptr) });

        unsafe { crate::globals::GLOBALS.get_mut() }.firstwin = prev_firstwin;
        unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage = prev_first_tabpage;
    }

    #[test]
    fn last_window_false_when_another_tabpage_exists() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = focusable_win(1);
        let win_ptr = &mut win as *mut WinT;
        let mut second_tp = crate::buffer_defs::TabpageT::default();
        let second_tp_ptr = &mut second_tp as *mut crate::buffer_defs::TabpageT;
        let mut tp = crate::buffer_defs::TabpageT { tp_next: second_tp_ptr, ..Default::default() };
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let prev_firstwin = unsafe { crate::globals::GLOBALS.get_mut() }.firstwin;
        let prev_first_tabpage = unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage;
        unsafe { crate::globals::GLOBALS.get_mut() }.firstwin = win_ptr;
        unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage = tp_ptr;

        assert!(!unsafe { last_window(win_ptr) });

        unsafe { crate::globals::GLOBALS.get_mut() }.firstwin = prev_firstwin;
        unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage = prev_first_tabpage;
    }

    #[test]
    fn last_window_false_when_one_window_itself_is_false() {
        let _lock = crate::globals::global_state_test_lock();
        let mut second = focusable_win(2);
        let second_ptr = &mut second as *mut WinT;
        let mut win = WinT { w_next: second_ptr, ..focusable_win(1) };
        let win_ptr = &mut win as *mut WinT;
        let mut tp =
            crate::buffer_defs::TabpageT { tp_next: std::ptr::null_mut(), ..Default::default() };
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let prev_firstwin = unsafe { crate::globals::GLOBALS.get_mut() }.firstwin;
        let prev_first_tabpage = unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage;
        unsafe { crate::globals::GLOBALS.get_mut() }.firstwin = win_ptr;
        unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage = tp_ptr;

        assert!(!unsafe { last_window(win_ptr) });

        unsafe { crate::globals::GLOBALS.get_mut() }.firstwin = prev_firstwin;
        unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage = prev_first_tabpage;
    }

    // ---- check_lnums / check_lnums_nested / reset_lnums ----

    /// RAII guard for `check_lnums`/`reset_lnums` tests: saves/restores
    /// every `GLOBALS` field these functions read
    /// (`first_tabpage`/`curtab`/`firstwin`/`curwin`/`curbuf`).
    struct CheckLnumsGuard {
        prev_first_tabpage: *mut crate::buffer_defs::TabpageT,
        prev_curtab: *mut crate::buffer_defs::TabpageT,
        prev_firstwin: *mut WinT,
        prev_curwin: *mut WinT,
        prev_curbuf: *mut crate::buffer_defs::BufT,
    }
    impl CheckLnumsGuard {
        fn set(
            first_tabpage: *mut crate::buffer_defs::TabpageT,
            curtab: *mut crate::buffer_defs::TabpageT,
            firstwin: *mut WinT,
            curwin: *mut WinT,
            curbuf: *mut crate::buffer_defs::BufT,
        ) -> Self {
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            let guard = CheckLnumsGuard {
                prev_first_tabpage: g.first_tabpage,
                prev_curtab: g.curtab,
                prev_firstwin: g.firstwin,
                prev_curwin: g.curwin,
                prev_curbuf: g.curbuf,
            };
            g.first_tabpage = first_tabpage;
            g.curtab = curtab;
            g.firstwin = firstwin;
            g.curwin = curwin;
            g.curbuf = curbuf;
            guard
        }
    }
    impl Drop for CheckLnumsGuard {
        fn drop(&mut self) {
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            g.first_tabpage = self.prev_first_tabpage;
            g.curtab = self.prev_curtab;
            g.firstwin = self.prev_firstwin;
            g.curwin = self.prev_curwin;
            g.curbuf = self.prev_curbuf;
        }
    }

    fn buf_with_line_count(line_count: i32) -> crate::buffer_defs::BufT {
        crate::buffer_defs::BufT {
            b_ml: crate::memline_defs::MemlineT { ml_line_count: line_count, ..Default::default() },
            ..Default::default()
        }
    }

    fn win_showing(buf: *mut crate::buffer_defs::BufT, lnum: i32, topline: i32) -> WinT {
        WinT {
            w_buffer: buf,
            w_cursor: crate::pos_defs::PosT { lnum, col: 0, coladd: 0 },
            w_topline: topline,
            ..Default::default()
        }
    }

    #[test]
    fn check_lnums_do_curwin_false_skips_the_current_window() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = buf_with_line_count(5);
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let mut win2 = win_showing(buf_ptr, 10, 1);
        let win2_ptr = &mut win2 as *mut WinT;
        let mut win1 = WinT { w_next: win2_ptr, ..win_showing(buf_ptr, 3, 1) };
        let win1_ptr = &mut win1 as *mut WinT;
        let mut tp = crate::buffer_defs::TabpageT { tp_firstwin: win1_ptr, ..Default::default() };
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = CheckLnumsGuard::set(tp_ptr, tp_ptr, win1_ptr, win1_ptr, buf_ptr);

        unsafe { check_lnums(false) };

        // win1 IS curwin, and do_curwin was false - completely untouched.
        assert_eq!(unsafe { &*win1_ptr }.w_cursor.lnum, 3);
        assert_eq!(unsafe { &*win1_ptr }.w_save_cursor.w_cursor_save.lnum, 0);

        // win2 is not curwin - clamped and saved.
        assert_eq!(unsafe { &*win2_ptr }.w_cursor.lnum, 5);
        assert_eq!(unsafe { &*win2_ptr }.w_save_cursor.w_cursor_save.lnum, 10);
        assert_eq!(unsafe { &*win2_ptr }.w_save_cursor.w_topline_save, 1);
        assert_eq!(unsafe { &*win2_ptr }.w_save_cursor.w_cursor_corr.lnum, 5);
        assert_eq!(unsafe { &*win2_ptr }.w_save_cursor.w_topline_corr, 1);
    }

    #[test]
    fn check_lnums_do_curwin_true_also_updates_the_current_window() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = buf_with_line_count(5);
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let mut win = win_showing(buf_ptr, 3, 1);
        let win_ptr = &mut win as *mut WinT;
        let mut tp = crate::buffer_defs::TabpageT { tp_firstwin: win_ptr, ..Default::default() };
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = CheckLnumsGuard::set(tp_ptr, tp_ptr, win_ptr, win_ptr, buf_ptr);

        unsafe { check_lnums(true) };

        assert_eq!(unsafe { &*win_ptr }.w_cursor.lnum, 3);
        assert_eq!(unsafe { &*win_ptr }.w_save_cursor.w_cursor_save.lnum, 3);
        assert_eq!(unsafe { &*win_ptr }.w_save_cursor.w_topline_save, 1);
        assert_eq!(unsafe { &*win_ptr }.w_save_cursor.w_cursor_corr.lnum, 3);
        assert_eq!(unsafe { &*win_ptr }.w_save_cursor.w_topline_corr, 1);
    }

    #[test]
    fn check_lnums_clamps_cursor_and_topline_when_out_of_range() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = buf_with_line_count(5);
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let mut win = win_showing(buf_ptr, 100, 200);
        let win_ptr = &mut win as *mut WinT;
        let mut tp = crate::buffer_defs::TabpageT { tp_firstwin: win_ptr, ..Default::default() };
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = CheckLnumsGuard::set(tp_ptr, tp_ptr, win_ptr, win_ptr, buf_ptr);

        unsafe { check_lnums(true) };

        assert_eq!(unsafe { &*win_ptr }.w_cursor.lnum, 5);
        assert_eq!(unsafe { &*win_ptr }.w_topline, 5);
        assert_eq!(unsafe { &*win_ptr }.w_save_cursor.w_cursor_save.lnum, 100);
        assert_eq!(unsafe { &*win_ptr }.w_save_cursor.w_topline_save, 200);
        assert_eq!(unsafe { &*win_ptr }.w_save_cursor.w_cursor_corr.lnum, 5);
        assert_eq!(unsafe { &*win_ptr }.w_save_cursor.w_topline_corr, 5);
    }

    #[test]
    fn check_lnums_skips_a_window_showing_a_different_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = buf_with_line_count(5);
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let mut other_buf = buf_with_line_count(5);
        let other_buf_ptr = &mut other_buf as *mut crate::buffer_defs::BufT;
        let mut win = win_showing(other_buf_ptr, 3, 1);
        let win_ptr = &mut win as *mut WinT;
        let mut tp = crate::buffer_defs::TabpageT { tp_firstwin: win_ptr, ..Default::default() };
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = CheckLnumsGuard::set(tp_ptr, tp_ptr, win_ptr, win_ptr, buf_ptr);

        unsafe { check_lnums(true) };

        // win shows other_buf, not curbuf - untouched.
        assert_eq!(unsafe { &*win_ptr }.w_save_cursor.w_cursor_save.lnum, 0);
    }

    #[test]
    fn check_lnums_walks_windows_in_every_tabpage_not_just_curtab() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = buf_with_line_count(5);
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let mut win2 = win_showing(buf_ptr, 100, 1);
        let win2_ptr = &mut win2 as *mut WinT;
        let mut tp2 = crate::buffer_defs::TabpageT { tp_firstwin: win2_ptr, ..Default::default() };
        let tp2_ptr = &mut tp2 as *mut crate::buffer_defs::TabpageT;
        let mut win1 = win_showing(buf_ptr, 3, 1);
        let win1_ptr = &mut win1 as *mut WinT;
        let mut tp1 =
            crate::buffer_defs::TabpageT { tp_next: tp2_ptr, tp_firstwin: win1_ptr, ..Default::default() };
        let tp1_ptr = &mut tp1 as *mut crate::buffer_defs::TabpageT;
        let _guard = CheckLnumsGuard::set(tp1_ptr, tp1_ptr, win1_ptr, win1_ptr, buf_ptr);

        unsafe { check_lnums(true) };

        // win1 is in curtab (tp1) - touched.
        assert_eq!(unsafe { &*win1_ptr }.w_save_cursor.w_cursor_save.lnum, 3);
        // win2 is in a DIFFERENT tabpage (tp2), but shows curbuf too -
        // FOR_ALL_TAB_WINDOWS walks every tabpage, so it's touched.
        assert_eq!(unsafe { &*win2_ptr }.w_cursor.lnum, 5);
        assert_eq!(unsafe { &*win2_ptr }.w_save_cursor.w_cursor_save.lnum, 100);
    }

    #[test]
    fn check_lnums_nested_does_not_resave_when_no_adjustment_is_needed() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = buf_with_line_count(5);
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let mut win = win_showing(buf_ptr, 3, 1);
        let win_ptr = &mut win as *mut WinT;
        let mut tp = crate::buffer_defs::TabpageT { tp_firstwin: win_ptr, ..Default::default() };
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = CheckLnumsGuard::set(tp_ptr, tp_ptr, win_ptr, win_ptr, buf_ptr);

        unsafe { check_lnums(true) };
        // Overwrite the corrected values with a sentinel to detect
        // whether the nested call below re-saves them.
        unsafe { &mut *win_ptr }.w_save_cursor.w_cursor_corr.lnum = 99;
        unsafe { &mut *win_ptr }.w_save_cursor.w_topline_corr = 99;

        // Cursor/topline are STILL within bounds - no adjustment
        // needed, and nested=true means neither branch re-saves.
        unsafe { check_lnums_nested(true) };

        assert_eq!(unsafe { &*win_ptr }.w_save_cursor.w_cursor_corr.lnum, 99);
        assert_eq!(unsafe { &*win_ptr }.w_save_cursor.w_topline_corr, 99);
    }

    #[test]
    fn check_lnums_nested_still_resaves_when_adjustment_is_needed() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = buf_with_line_count(5);
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let mut win = win_showing(buf_ptr, 3, 1);
        let win_ptr = &mut win as *mut WinT;
        let mut tp = crate::buffer_defs::TabpageT { tp_firstwin: win_ptr, ..Default::default() };
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = CheckLnumsGuard::set(tp_ptr, tp_ptr, win_ptr, win_ptr, buf_ptr);

        unsafe { check_lnums(true) };
        // Shrink the buffer so the SAME cursor now needs clamping.
        unsafe { &mut *buf_ptr }.b_ml.ml_line_count = 2;
        unsafe { &mut *win_ptr }.w_save_cursor.w_cursor_corr.lnum = 99;

        unsafe { check_lnums_nested(true) };

        // need_adjust was true this time (3 > 2), so the corrected
        // value IS re-saved even though nested=true.
        assert_eq!(unsafe { &*win_ptr }.w_cursor.lnum, 2);
        assert_eq!(unsafe { &*win_ptr }.w_save_cursor.w_cursor_corr.lnum, 2);
    }

    #[test]
    fn reset_lnums_restores_cursor_and_topline_when_autocmd_did_not_change_them() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = buf_with_line_count(20);
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let mut win = WinT {
            w_buffer: buf_ptr,
            w_cursor: crate::pos_defs::PosT { lnum: 5, col: 0, coladd: 0 },
            w_topline: 3,
            w_valid: i32::from(crate::buffer_defs::w_valid::VALID_TOPLINE),
            w_save_cursor: crate::buffer_defs::PosSaveT {
                w_topline_save: 7,
                w_topline_corr: 3,
                w_cursor_save: crate::pos_defs::PosT { lnum: 10, col: 0, coladd: 0 },
                w_cursor_corr: crate::pos_defs::PosT { lnum: 5, col: 0, coladd: 0 },
            },
            ..Default::default()
        };
        let win_ptr = &mut win as *mut WinT;
        let mut tp = crate::buffer_defs::TabpageT { tp_firstwin: win_ptr, ..Default::default() };
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = CheckLnumsGuard::set(tp_ptr, tp_ptr, win_ptr, win_ptr, buf_ptr);

        unsafe { reset_lnums() };

        assert_eq!(unsafe { &*win_ptr }.w_cursor.lnum, 10);
        assert_eq!(unsafe { &*win_ptr }.w_topline, 7);
        assert_ne!(
            unsafe { &*win_ptr }.w_valid & i32::from(crate::buffer_defs::w_valid::VALID_TOPLINE),
            0,
            "topline_save (7) <= line_count (20) - VALID_TOPLINE must stay set"
        );
    }

    #[test]
    fn reset_lnums_does_not_restore_when_autocmd_changed_the_cursor() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = buf_with_line_count(20);
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let mut win = WinT {
            w_buffer: buf_ptr,
            // The autocmd moved the cursor to line 8 - no longer
            // equal to w_cursor_corr (5), so no restore happens.
            w_cursor: crate::pos_defs::PosT { lnum: 8, col: 0, coladd: 0 },
            w_topline: 3,
            w_save_cursor: crate::buffer_defs::PosSaveT {
                w_topline_save: 7,
                w_topline_corr: 3,
                w_cursor_save: crate::pos_defs::PosT { lnum: 10, col: 0, coladd: 0 },
                w_cursor_corr: crate::pos_defs::PosT { lnum: 5, col: 0, coladd: 0 },
            },
            ..Default::default()
        };
        let win_ptr = &mut win as *mut WinT;
        let mut tp = crate::buffer_defs::TabpageT { tp_firstwin: win_ptr, ..Default::default() };
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = CheckLnumsGuard::set(tp_ptr, tp_ptr, win_ptr, win_ptr, buf_ptr);

        unsafe { reset_lnums() };

        assert_eq!(unsafe { &*win_ptr }.w_cursor.lnum, 8);
    }

    #[test]
    fn reset_lnums_does_not_restore_when_cursor_save_lnum_is_zero() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = buf_with_line_count(20);
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let mut win = WinT {
            w_buffer: buf_ptr,
            w_cursor: crate::pos_defs::PosT { lnum: 5, col: 0, coladd: 0 },
            w_topline: 3,
            w_save_cursor: crate::buffer_defs::PosSaveT {
                // w_cursor_save.lnum == 0: check_lnums was never
                // meaningfully called for this window - no restore.
                w_topline_save: 0,
                w_topline_corr: 3,
                w_cursor_save: crate::pos_defs::PosT::default(),
                w_cursor_corr: crate::pos_defs::PosT { lnum: 5, col: 0, coladd: 0 },
            },
            ..Default::default()
        };
        let win_ptr = &mut win as *mut WinT;
        let mut tp = crate::buffer_defs::TabpageT { tp_firstwin: win_ptr, ..Default::default() };
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = CheckLnumsGuard::set(tp_ptr, tp_ptr, win_ptr, win_ptr, buf_ptr);

        unsafe { reset_lnums() };

        assert_eq!(unsafe { &*win_ptr }.w_cursor.lnum, 5);
        assert_eq!(unsafe { &*win_ptr }.w_topline, 3);
    }

    #[test]
    fn reset_lnums_clears_valid_topline_when_saved_topline_exceeds_new_line_count() {
        let _lock = crate::globals::global_state_test_lock();
        // Buffer shrunk to 5 lines since check_lnums saved topline 50.
        let mut buf = buf_with_line_count(5);
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let mut win = WinT {
            w_buffer: buf_ptr,
            w_cursor: crate::pos_defs::PosT { lnum: 5, col: 0, coladd: 0 },
            // topline_corr != w_topline (2 != 3) - the topline branch
            // itself does not fire, isolating the VALID_TOPLINE check.
            w_topline: 3,
            w_valid: i32::from(crate::buffer_defs::w_valid::VALID_TOPLINE),
            w_save_cursor: crate::buffer_defs::PosSaveT {
                w_topline_save: 50,
                w_topline_corr: 2,
                w_cursor_save: crate::pos_defs::PosT { lnum: 5, col: 0, coladd: 0 },
                w_cursor_corr: crate::pos_defs::PosT { lnum: 5, col: 0, coladd: 0 },
            },
            ..Default::default()
        };
        let win_ptr = &mut win as *mut WinT;
        let mut tp = crate::buffer_defs::TabpageT { tp_firstwin: win_ptr, ..Default::default() };
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = CheckLnumsGuard::set(tp_ptr, tp_ptr, win_ptr, win_ptr, buf_ptr);

        unsafe { reset_lnums() };

        // Topline itself is untouched (topline_corr didn't match)...
        assert_eq!(unsafe { &*win_ptr }.w_topline, 3);
        // ...but VALID_TOPLINE is cleared regardless, since
        // w_topline_save (50) > the buffer's new line_count (5).
        assert_eq!(
            unsafe { &*win_ptr }.w_valid & i32::from(crate::buffer_defs::w_valid::VALID_TOPLINE),
            0
        );
    }

    #[test]
    fn reset_lnums_skips_a_window_showing_a_different_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = buf_with_line_count(5);
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let mut other_buf = buf_with_line_count(5);
        let other_buf_ptr = &mut other_buf as *mut crate::buffer_defs::BufT;
        let mut win = WinT {
            w_buffer: other_buf_ptr,
            w_cursor: crate::pos_defs::PosT { lnum: 5, col: 0, coladd: 0 },
            w_topline: 3,
            w_save_cursor: crate::buffer_defs::PosSaveT {
                w_topline_save: 7,
                w_topline_corr: 3,
                w_cursor_save: crate::pos_defs::PosT { lnum: 10, col: 0, coladd: 0 },
                w_cursor_corr: crate::pos_defs::PosT { lnum: 5, col: 0, coladd: 0 },
            },
            ..Default::default()
        };
        let win_ptr = &mut win as *mut WinT;
        let mut tp = crate::buffer_defs::TabpageT { tp_firstwin: win_ptr, ..Default::default() };
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = CheckLnumsGuard::set(tp_ptr, tp_ptr, win_ptr, win_ptr, buf_ptr);

        unsafe { reset_lnums() };

        // win shows other_buf, not curbuf - reset_lnums must not
        // touch it even though w_cursor_save.lnum != 0.
        assert_eq!(unsafe { &*win_ptr }.w_cursor.lnum, 5);
    }

    // ---- frame_check_height / frame_check_width ----

    #[test]
    fn frame_check_height_true_for_a_single_leaf_matching_height() {
        let leaf =
            crate::buffer_defs::FrameT { fr_layout: crate::buffer_defs::FR_LEAF, fr_height: 10, ..Default::default() };
        let leaf_ptr = &leaf as *const crate::buffer_defs::FrameT;

        assert!(unsafe { frame_check_height(leaf_ptr, 10) });
    }

    #[test]
    fn frame_check_height_false_for_a_single_leaf_with_a_different_height() {
        let leaf =
            crate::buffer_defs::FrameT { fr_layout: crate::buffer_defs::FR_LEAF, fr_height: 10, ..Default::default() };
        let leaf_ptr = &leaf as *const crate::buffer_defs::FrameT;

        assert!(!unsafe { frame_check_height(leaf_ptr, 5) });
    }

    #[test]
    fn frame_check_height_row_requires_every_child_to_match() {
        let mut child2 =
            crate::buffer_defs::FrameT { fr_layout: crate::buffer_defs::FR_LEAF, fr_height: 10, ..Default::default() };
        let child2_ptr = &mut child2 as *mut crate::buffer_defs::FrameT;
        let mut child1 = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_LEAF,
            fr_height: 10,
            fr_next: child2_ptr,
            ..Default::default()
        };
        let child1_ptr = &mut child1 as *mut crate::buffer_defs::FrameT;
        let row = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_ROW,
            fr_height: 10,
            fr_child: child1_ptr,
            ..Default::default()
        };
        let row_ptr = &row as *const crate::buffer_defs::FrameT;

        assert!(unsafe { frame_check_height(row_ptr, 10) });

        // Make ONE child disagree - the whole row is no longer
        // "every frame has this exact height".
        unsafe { &mut *child2_ptr }.fr_height = 9;
        assert!(!unsafe { frame_check_height(row_ptr, 10) });
    }

    #[test]
    fn frame_check_height_col_only_checks_the_top_frame_itself() {
        // A FR_COL's own children are NOT walked by frame_check_height
        // (only FR_ROW children are, per the original's own `if
        // (topfrp->fr_layout == FR_ROW)` guard) - a mismatched child
        // inside a FR_COL is invisible to this specific check.
        let mut child =
            crate::buffer_defs::FrameT { fr_layout: crate::buffer_defs::FR_LEAF, fr_height: 3, ..Default::default() };
        let child_ptr = &mut child as *mut crate::buffer_defs::FrameT;
        let col = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_COL,
            fr_height: 10,
            fr_child: child_ptr,
            ..Default::default()
        };
        let col_ptr = &col as *const crate::buffer_defs::FrameT;

        assert!(unsafe { frame_check_height(col_ptr, 10) });
    }

    #[test]
    fn frame_check_width_col_requires_every_child_to_match() {
        let mut child2 =
            crate::buffer_defs::FrameT { fr_layout: crate::buffer_defs::FR_LEAF, fr_width: 20, ..Default::default() };
        let child2_ptr = &mut child2 as *mut crate::buffer_defs::FrameT;
        let mut child1 = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_LEAF,
            fr_width: 20,
            fr_next: child2_ptr,
            ..Default::default()
        };
        let child1_ptr = &mut child1 as *mut crate::buffer_defs::FrameT;
        let col = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_COL,
            fr_width: 20,
            fr_child: child1_ptr,
            ..Default::default()
        };
        let col_ptr = &col as *const crate::buffer_defs::FrameT;

        assert!(unsafe { frame_check_width(col_ptr, 20) });

        unsafe { &mut *child2_ptr }.fr_width = 19;
        assert!(!unsafe { frame_check_width(col_ptr, 20) });
    }

    #[test]
    fn frame_check_width_row_only_checks_the_top_frame_itself() {
        let mut child =
            crate::buffer_defs::FrameT { fr_layout: crate::buffer_defs::FR_LEAF, fr_width: 3, ..Default::default() };
        let child_ptr = &mut child as *mut crate::buffer_defs::FrameT;
        let row = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_ROW,
            fr_width: 20,
            fr_child: child_ptr,
            ..Default::default()
        };
        let row_ptr = &row as *const crate::buffer_defs::FrameT;

        assert!(unsafe { frame_check_width(row_ptr, 20) });
    }

    #[test]
    fn frame_check_width_false_when_the_top_frame_itself_disagrees() {
        let leaf =
            crate::buffer_defs::FrameT { fr_layout: crate::buffer_defs::FR_LEAF, fr_width: 20, ..Default::default() };
        let leaf_ptr = &leaf as *const crate::buffer_defs::FrameT;

        assert!(!unsafe { frame_check_width(leaf_ptr, 21) });
    }

    // ---- check_colorcolumn ----

    fn buf_with_tw(tw: crate::types_defs::OptInt) -> crate::buffer_defs::BufT {
        crate::buffer_defs::BufT { b_p_tw: tw, ..Default::default() }
    }

    #[test]
    fn check_colorcolumn_buffer_was_closed_returns_true_without_changing_anything() {
        let mut win = WinT {
            w_buffer: std::ptr::null_mut(),
            w_p_cc_cols: Some(vec![1, 2]),
            ..Default::default()
        };

        assert!(unsafe { check_colorcolumn(Some(b"5"), Some(&mut win)) });
        assert_eq!(win.w_p_cc_cols, Some(vec![1, 2]));
    }

    #[test]
    fn check_colorcolumn_only_parses_when_wp_is_none() {
        assert!(unsafe { check_colorcolumn(Some(b"5,10"), None) });
        assert!(!unsafe { check_colorcolumn(Some(b"x"), None) });
    }

    #[test]
    fn check_colorcolumn_parses_plain_digit_columns_1_based_to_0_based() {
        let mut buf = buf_with_tw(0);
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let mut win = WinT { w_buffer: buf_ptr, ..Default::default() };

        assert!(unsafe { check_colorcolumn(Some(b"5,10"), Some(&mut win)) });
        assert_eq!(win.w_p_cc_cols, Some(vec![4, 9]));
    }

    #[test]
    fn check_colorcolumn_relative_plus_minus_add_to_textwidth() {
        let mut buf = buf_with_tw(80);
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let mut win = WinT { w_buffer: buf_ptr, ..Default::default() };

        // "+1" -> 1 + 80 = 81 -> 0-based 80. "-1" -> -1 + 80 = 79 ->
        // 0-based 78. Sorted ascending: [78, 80].
        assert!(unsafe { check_colorcolumn(Some(b"+1,-1"), Some(&mut win)) });
        assert_eq!(win.w_p_cc_cols, Some(vec![78, 80]));
    }

    #[test]
    fn check_colorcolumn_skips_relative_column_when_textwidth_is_zero() {
        let mut buf = buf_with_tw(0);
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let mut win = WinT { w_buffer: buf_ptr, ..Default::default() };

        assert!(unsafe { check_colorcolumn(Some(b"+1"), Some(&mut win)) });
        assert_eq!(win.w_p_cc_cols, None);
    }

    #[test]
    fn check_colorcolumn_skips_relative_column_that_goes_negative() {
        let mut buf = buf_with_tw(5);
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let mut win = WinT { w_buffer: buf_ptr, ..Default::default() };

        // "-100" -> -100 + 5 = -95 < 0 -> silently dropped.
        assert!(unsafe { check_colorcolumn(Some(b"-100"), Some(&mut win)) });
        assert_eq!(win.w_p_cc_cols, None);
    }

    #[test]
    fn check_colorcolumn_sorts_and_dedups() {
        let mut buf = buf_with_tw(0);
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let mut win = WinT { w_buffer: buf_ptr, ..Default::default() };

        assert!(unsafe { check_colorcolumn(Some(b"10,5,10,5"), Some(&mut win)) });
        assert_eq!(win.w_p_cc_cols, Some(vec![4, 9]));
    }

    #[test]
    fn check_colorcolumn_invalid_leading_character_returns_false() {
        let mut buf = buf_with_tw(0);
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let mut win = WinT { w_buffer: buf_ptr, ..Default::default() };

        assert!(!unsafe { check_colorcolumn(Some(b"x"), Some(&mut win)) });
    }

    #[test]
    fn check_colorcolumn_missing_digit_after_sign_returns_false() {
        let mut buf = buf_with_tw(0);
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let mut win = WinT { w_buffer: buf_ptr, ..Default::default() };

        assert!(!unsafe { check_colorcolumn(Some(b"+"), Some(&mut win)) });
    }

    #[test]
    fn check_colorcolumn_missing_comma_between_items_returns_false() {
        let mut buf = buf_with_tw(0);
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let mut win = WinT { w_buffer: buf_ptr, ..Default::default() };

        assert!(!unsafe { check_colorcolumn(Some(b"5x10"), Some(&mut win)) });
    }

    #[test]
    fn check_colorcolumn_trailing_comma_returns_false() {
        let mut buf = buf_with_tw(0);
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let mut win = WinT { w_buffer: buf_ptr, ..Default::default() };

        assert!(!unsafe { check_colorcolumn(Some(b"80,"), Some(&mut win)) });
    }

    #[test]
    fn check_colorcolumn_empty_string_clears_a_previous_value() {
        let mut buf = buf_with_tw(0);
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let mut win =
            WinT { w_buffer: buf_ptr, w_p_cc_cols: Some(vec![1, 2]), ..Default::default() };

        assert!(unsafe { check_colorcolumn(Some(b""), Some(&mut win)) });
        assert_eq!(win.w_p_cc_cols, None);
    }

    #[test]
    fn check_colorcolumn_uses_wp_own_wo_cc_when_cc_is_none() {
        let mut buf = buf_with_tw(0);
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let mut win = WinT {
            w_buffer: buf_ptr,
            w_onebuf_opt: crate::buffer_defs::WinoptT { wo_cc: Some(b"3".to_vec()), ..Default::default() },
            ..Default::default()
        };

        assert!(unsafe { check_colorcolumn(None, Some(&mut win)) });
        assert_eq!(win.w_p_cc_cols, Some(vec![2]));
    }

    #[test]
    fn check_colorcolumn_uses_empty_when_wo_cc_and_cc_are_both_none() {
        let mut buf = buf_with_tw(0);
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let mut win = WinT { w_buffer: buf_ptr, w_p_cc_cols: Some(vec![1]), ..Default::default() };

        assert!(unsafe { check_colorcolumn(None, Some(&mut win)) });
        assert_eq!(win.w_p_cc_cols, None);
    }

    // ---- lastwin_nofloating ----

    #[test]
    fn lastwin_nofloating_null_tp_uses_globals_lastwin() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = focusable_win(1);
        let win_ptr = &mut win as *mut WinT;
        let prev_lastwin = unsafe { crate::globals::GLOBALS.get_mut() }.lastwin;
        let prev_curtab = unsafe { crate::globals::GLOBALS.get_mut() }.curtab;
        unsafe { crate::globals::GLOBALS.get_mut() }.lastwin = win_ptr;
        unsafe { crate::globals::GLOBALS.get_mut() }.curtab = std::ptr::null_mut();

        assert_eq!(unsafe { lastwin_nofloating(std::ptr::null()) }, win_ptr);

        unsafe { crate::globals::GLOBALS.get_mut() }.lastwin = prev_lastwin;
        unsafe { crate::globals::GLOBALS.get_mut() }.curtab = prev_curtab;
    }

    #[test]
    fn lastwin_nofloating_skips_floating_windows_via_w_prev() {
        let _lock = crate::globals::global_state_test_lock();
        let mut non_floating = focusable_win(1);
        let non_floating_ptr = &mut non_floating as *mut WinT;
        let mut floating = WinT { w_floating: true, w_prev: non_floating_ptr, ..focusable_win(2) };
        let floating_ptr = &mut floating as *mut WinT;
        let prev_lastwin = unsafe { crate::globals::GLOBALS.get_mut() }.lastwin;
        let prev_curtab = unsafe { crate::globals::GLOBALS.get_mut() }.curtab;
        unsafe { crate::globals::GLOBALS.get_mut() }.lastwin = floating_ptr;
        unsafe { crate::globals::GLOBALS.get_mut() }.curtab = std::ptr::null_mut();

        assert_eq!(unsafe { lastwin_nofloating(std::ptr::null()) }, non_floating_ptr);

        unsafe { crate::globals::GLOBALS.get_mut() }.lastwin = prev_lastwin;
        unsafe { crate::globals::GLOBALS.get_mut() }.curtab = prev_curtab;
    }

    #[test]
    fn lastwin_nofloating_uses_tp_own_tp_lastwin_when_non_curtab() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = focusable_win(1);
        let win_ptr = &mut win as *mut WinT;
        let mut other_tp = crate::buffer_defs::TabpageT { tp_lastwin: win_ptr, ..Default::default() };
        let other_tp_ptr = &mut other_tp as *mut crate::buffer_defs::TabpageT;
        let mut cur_tp = crate::buffer_defs::TabpageT::default();
        let cur_tp_ptr = &mut cur_tp as *mut crate::buffer_defs::TabpageT;
        let prev_curtab = unsafe { crate::globals::GLOBALS.get_mut() }.curtab;
        unsafe { crate::globals::GLOBALS.get_mut() }.curtab = cur_tp_ptr;

        // curtab is cur_tp_ptr, NOT other_tp_ptr - proving other_tp's
        // own tp_lastwin is used, not GLOBALS.lastwin.
        assert_eq!(unsafe { lastwin_nofloating(other_tp_ptr) }, win_ptr);

        unsafe { crate::globals::GLOBALS.get_mut() }.curtab = prev_curtab;
    }

    #[test]
    #[cfg(debug_assertions)]
    fn lastwin_nofloating_debug_panics_when_tp_equals_curtab() {
        let _lock = crate::globals::global_state_test_lock();
        // tp_lastwin points at a real, non-floating window so this
        // stays memory-safe even if the debug_assert! were ever
        // weakened - the test's own point is the panic, not a crash.
        let mut win = focusable_win(1);
        let win_ptr = &mut win as *mut WinT;
        let mut tp = crate::buffer_defs::TabpageT { tp_lastwin: win_ptr, ..Default::default() };
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let prev_curtab = unsafe { crate::globals::GLOBALS.get_mut() }.curtab;
        unsafe { crate::globals::GLOBALS.get_mut() }.curtab = tp_ptr;

        // catch_unwind (rather than #[should_panic]) so curtab is
        // always restored before this test returns, even though the
        // call panics - see move.rs's own established precedent for
        // this exact pattern. This whole test is #[cfg(debug_assertions)]
        // since the panic itself comes from a debug_assert!, which
        // compiles out entirely in --release (a plain #[should_panic]
        // would spuriously fail there).
        let result =
            std::panic::catch_unwind(|| unsafe { lastwin_nofloating(tp_ptr.cast_const()) });

        unsafe { crate::globals::GLOBALS.get_mut() }.curtab = prev_curtab;

        assert!(result.is_err(), "expected a debug_assert! panic");
    }

    // ---- last_stl_height ----

    #[test]
    fn last_stl_height_zero_when_laststatus_is_zero() {
        let _lock = crate::globals::global_state_test_lock();
        let prev_ls = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ls;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ls = 0;

        assert_eq!(unsafe { last_stl_height(false) }, 0);
        assert_eq!(unsafe { last_stl_height(true) }, 0);

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ls = prev_ls;
    }

    #[test]
    fn last_stl_height_full_when_laststatus_is_greater_than_one() {
        let _lock = crate::globals::global_state_test_lock();
        let prev_ls = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ls;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ls = 2;

        // Always STATUS_HEIGHT with laststatus=2, regardless of
        // window count or morewin.
        assert_eq!(unsafe { last_stl_height(false) }, STATUS_HEIGHT);

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ls = prev_ls;
    }

    #[test]
    fn last_stl_height_laststatus_one_needs_morewin_or_multiple_windows() {
        let _lock = crate::globals::global_state_test_lock();
        let prev_ls = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ls;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ls = 1;

        // A single window, morewin=false: one_window() is true, so
        // !one_window() is false, and morewin is false -> no status
        // line reserved.
        let mut win = focusable_win(1);
        let win_ptr = &mut win as *mut WinT;
        let prev_firstwin = unsafe { crate::globals::GLOBALS.get_mut() }.firstwin;
        unsafe { crate::globals::GLOBALS.get_mut() }.firstwin = win_ptr;
        assert_eq!(unsafe { last_stl_height(false) }, 0);

        // Same single-window setup, but morewin=true - always shown,
        // since a second window is about to be added.
        assert_eq!(unsafe { last_stl_height(true) }, STATUS_HEIGHT);

        // Two windows, morewin=false: one_window() is now false, so
        // !one_window() is true -> shown.
        let mut second = focusable_win(2);
        let second_ptr = &mut second as *mut WinT;
        unsafe { &mut *win_ptr }.w_next = second_ptr;
        assert_eq!(unsafe { last_stl_height(false) }, STATUS_HEIGHT);

        unsafe { crate::globals::GLOBALS.get_mut() }.firstwin = prev_firstwin;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ls = prev_ls;
    }

    // ---- make_win_info_dict ----

    #[test]
    fn make_win_info_dict_builds_all_six_keys_with_the_given_values() {
        let _lock = crate::globals::global_state_test_lock();
        let d = make_win_info_dict(10, 20, 30, 40, 50, 60).expect("should succeed");

        // SAFETY: `d` is a freshly-allocated, exclusively-owned dict.
        unsafe {
            assert_eq!(crate::eval::typval::tv_dict_get_number(Some(&mut *d), b"width"), 10);
            assert_eq!(crate::eval::typval::tv_dict_get_number(Some(&mut *d), b"height"), 20);
            assert_eq!(crate::eval::typval::tv_dict_get_number(Some(&mut *d), b"topline"), 30);
            assert_eq!(crate::eval::typval::tv_dict_get_number(Some(&mut *d), b"topfill"), 40);
            assert_eq!(crate::eval::typval::tv_dict_get_number(Some(&mut *d), b"leftcol"), 50);
            assert_eq!(crate::eval::typval::tv_dict_get_number(Some(&mut *d), b"skipcol"), 60);
            assert_eq!((*d).dv_refcount, 1);
            crate::eval::typval::tv_dict_unref(d);
        }
    }

    #[test]
    fn make_win_info_dict_handles_zero_and_negative_values() {
        let _lock = crate::globals::global_state_test_lock();
        let d = make_win_info_dict(0, 0, 1, 0, -5, -1).expect("should succeed");

        // SAFETY: `d` is a freshly-allocated, exclusively-owned dict.
        unsafe {
            assert_eq!(crate::eval::typval::tv_dict_get_number(Some(&mut *d), b"width"), 0);
            assert_eq!(crate::eval::typval::tv_dict_get_number(Some(&mut *d), b"leftcol"), -5);
            assert_eq!(crate::eval::typval::tv_dict_get_number(Some(&mut *d), b"skipcol"), -1);
            crate::eval::typval::tv_dict_unref(d);
        }
    }

    // ---- alt_tabpage ----

    /// RAII guard for `alt_tabpage` tests: saves/restores every
    /// `GLOBALS`/`OPTION_VARS` field it reads.
    struct AltTabpageGuard {
        prev_first_tabpage: *mut crate::buffer_defs::TabpageT,
        prev_curtab: *mut crate::buffer_defs::TabpageT,
        prev_lastused_tabpage: *mut crate::buffer_defs::TabpageT,
        prev_tcl_flags: u32,
    }
    impl AltTabpageGuard {
        fn set(
            first_tabpage: *mut crate::buffer_defs::TabpageT,
            curtab: *mut crate::buffer_defs::TabpageT,
            lastused_tabpage: *mut crate::buffer_defs::TabpageT,
            tcl_flags: u32,
        ) -> Self {
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            let guard = AltTabpageGuard {
                prev_first_tabpage: g.first_tabpage,
                prev_curtab: g.curtab,
                prev_lastused_tabpage: g.lastused_tabpage,
                prev_tcl_flags: opts.tcl_flags,
            };
            g.first_tabpage = first_tabpage;
            g.curtab = curtab;
            g.lastused_tabpage = lastused_tabpage;
            opts.tcl_flags = tcl_flags;
            guard
        }
    }
    impl Drop for AltTabpageGuard {
        fn drop(&mut self) {
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            g.first_tabpage = self.prev_first_tabpage;
            g.curtab = self.prev_curtab;
            g.lastused_tabpage = self.prev_lastused_tabpage;
            opts.tcl_flags = self.prev_tcl_flags;
        }
    }

    /// Builds a 3-tabpage chain `tp1 (first) -> tp2 -> tp3 (last)`.
    fn three_tabpage_chain() -> (
        crate::buffer_defs::TabpageT,
        crate::buffer_defs::TabpageT,
        crate::buffer_defs::TabpageT,
    ) {
        let tp3 = crate::buffer_defs::TabpageT { tp_next: std::ptr::null_mut(), ..Default::default() };
        (crate::buffer_defs::TabpageT::default(), crate::buffer_defs::TabpageT::default(), tp3)
    }

    #[test]
    fn alt_tabpage_uses_lastused_when_flag_set_and_valid() {
        let _lock = crate::globals::global_state_test_lock();
        let (mut tp1, mut tp2, mut tp3) = three_tabpage_chain();
        let tp3_ptr = &mut tp3 as *mut crate::buffer_defs::TabpageT;
        tp2.tp_next = tp3_ptr;
        let tp2_ptr = &mut tp2 as *mut crate::buffer_defs::TabpageT;
        tp1.tp_next = tp2_ptr;
        let tp1_ptr = &mut tp1 as *mut crate::buffer_defs::TabpageT;
        let _guard = AltTabpageGuard::set(
            tp1_ptr,
            tp2_ptr,
            tp3_ptr,
            crate::option_vars::opt_tcl_flag::USELAST,
        );

        assert_eq!(unsafe { alt_tabpage() }, tp3_ptr);
    }

    #[test]
    fn alt_tabpage_ignores_lastused_when_flag_not_set() {
        let _lock = crate::globals::global_state_test_lock();
        let (mut tp1, mut tp2, mut tp3) = three_tabpage_chain();
        let tp3_ptr = &mut tp3 as *mut crate::buffer_defs::TabpageT;
        tp2.tp_next = tp3_ptr;
        let tp2_ptr = &mut tp2 as *mut crate::buffer_defs::TabpageT;
        tp1.tp_next = tp2_ptr;
        let tp1_ptr = &mut tp1 as *mut crate::buffer_defs::TabpageT;
        // lastused_tabpage is valid (tp3), but USELAST is NOT set -
        // must fall through to the forward/backward logic instead.
        let _guard = AltTabpageGuard::set(tp1_ptr, tp1_ptr, tp3_ptr, 0);

        // curtab is tp1 (== first_tabpage), forward -> tp1.tp_next = tp2.
        assert_eq!(unsafe { alt_tabpage() }, tp2_ptr);
    }

    #[test]
    fn alt_tabpage_ignores_lastused_when_not_a_valid_tabpage() {
        let _lock = crate::globals::global_state_test_lock();
        let (mut tp1, mut tp2, mut tp3) = three_tabpage_chain();
        let tp3_ptr = &mut tp3 as *mut crate::buffer_defs::TabpageT;
        tp2.tp_next = tp3_ptr;
        let tp2_ptr = &mut tp2 as *mut crate::buffer_defs::TabpageT;
        tp1.tp_next = tp2_ptr;
        let tp1_ptr = &mut tp1 as *mut crate::buffer_defs::TabpageT;
        // A standalone tabpage, never linked into the first_tabpage
        // chain - valid_tabpage() must report it as NOT valid.
        let mut unrelated = crate::buffer_defs::TabpageT::default();
        let unrelated_ptr = &mut unrelated as *mut crate::buffer_defs::TabpageT;
        let _guard = AltTabpageGuard::set(
            tp1_ptr,
            tp1_ptr,
            unrelated_ptr,
            crate::option_vars::opt_tcl_flag::USELAST,
        );

        assert_eq!(unsafe { alt_tabpage() }, tp2_ptr);
    }

    #[test]
    fn alt_tabpage_forward_when_no_left_flag_and_curtab_has_a_next() {
        let _lock = crate::globals::global_state_test_lock();
        let (mut tp1, mut tp2, mut tp3) = three_tabpage_chain();
        let tp3_ptr = &mut tp3 as *mut crate::buffer_defs::TabpageT;
        tp2.tp_next = tp3_ptr;
        let tp2_ptr = &mut tp2 as *mut crate::buffer_defs::TabpageT;
        tp1.tp_next = tp2_ptr;
        let tp1_ptr = &mut tp1 as *mut crate::buffer_defs::TabpageT;
        let _guard = AltTabpageGuard::set(tp1_ptr, tp2_ptr, std::ptr::null_mut(), 0);

        // curtab = tp2, no LEFT flag -> forward -> tp2.tp_next = tp3.
        assert_eq!(unsafe { alt_tabpage() }, tp3_ptr);
    }

    #[test]
    fn alt_tabpage_forward_when_left_flag_set_but_curtab_is_first_tabpage() {
        let _lock = crate::globals::global_state_test_lock();
        let (mut tp1, mut tp2, mut tp3) = three_tabpage_chain();
        let tp3_ptr = &mut tp3 as *mut crate::buffer_defs::TabpageT;
        tp2.tp_next = tp3_ptr;
        let tp2_ptr = &mut tp2 as *mut crate::buffer_defs::TabpageT;
        tp1.tp_next = tp2_ptr;
        let tp1_ptr = &mut tp1 as *mut crate::buffer_defs::TabpageT;
        let _guard = AltTabpageGuard::set(
            tp1_ptr,
            tp1_ptr,
            std::ptr::null_mut(),
            crate::option_vars::opt_tcl_flag::LEFT,
        );

        // curtab = tp1 = first_tabpage - forward despite the LEFT
        // flag, since "curtab == first_tabpage" is the OR condition.
        assert_eq!(unsafe { alt_tabpage() }, tp2_ptr);
    }

    #[test]
    fn alt_tabpage_backward_when_left_flag_set_and_curtab_is_not_first() {
        let _lock = crate::globals::global_state_test_lock();
        let (mut tp1, mut tp2, mut tp3) = three_tabpage_chain();
        let tp3_ptr = &mut tp3 as *mut crate::buffer_defs::TabpageT;
        tp2.tp_next = tp3_ptr;
        let tp2_ptr = &mut tp2 as *mut crate::buffer_defs::TabpageT;
        tp1.tp_next = tp2_ptr;
        let tp1_ptr = &mut tp1 as *mut crate::buffer_defs::TabpageT;
        let _guard = AltTabpageGuard::set(
            tp1_ptr,
            tp2_ptr,
            std::ptr::null_mut(),
            crate::option_vars::opt_tcl_flag::LEFT,
        );

        // curtab = tp2 (not first), LEFT flag set, tp2 has a next
        // (tp3) - backward: walk from tp1 until tp.tp_next == tp2 ->
        // tp1 itself.
        assert_eq!(unsafe { alt_tabpage() }, tp1_ptr);
    }

    #[test]
    fn alt_tabpage_backward_when_curtab_has_no_next_regardless_of_flags() {
        let _lock = crate::globals::global_state_test_lock();
        let (mut tp1, mut tp2, mut tp3) = three_tabpage_chain();
        let tp3_ptr = &mut tp3 as *mut crate::buffer_defs::TabpageT;
        tp2.tp_next = tp3_ptr;
        let tp2_ptr = &mut tp2 as *mut crate::buffer_defs::TabpageT;
        tp1.tp_next = tp2_ptr;
        let tp1_ptr = &mut tp1 as *mut crate::buffer_defs::TabpageT;
        // curtab = tp3, the LAST tabpage (tp_next is null) - forced
        // backward even with no LEFT flag at all.
        let _guard = AltTabpageGuard::set(tp1_ptr, tp3_ptr, std::ptr::null_mut(), 0);

        // Walk from tp1 until tp.tp_next == tp3 -> tp2.
        assert_eq!(unsafe { alt_tabpage() }, tp2_ptr);
    }

    // ---- frame_append / frame_insert / frame_remove ----

    #[test]
    fn frame_append_splices_into_the_middle_of_a_list() {
        let mut c = crate::buffer_defs::FrameT::default();
        let c_ptr = &mut c as *mut crate::buffer_defs::FrameT;
        let mut a =
            crate::buffer_defs::FrameT { fr_next: c_ptr, ..Default::default() };
        let a_ptr = &mut a as *mut crate::buffer_defs::FrameT;
        unsafe { &mut *c_ptr }.fr_prev = a_ptr;
        let mut b = crate::buffer_defs::FrameT::default();
        let b_ptr = &mut b as *mut crate::buffer_defs::FrameT;

        unsafe { frame_append(a_ptr, b_ptr) };

        assert_eq!(unsafe { &*a_ptr }.fr_next, b_ptr);
        assert_eq!(unsafe { &*b_ptr }.fr_prev, a_ptr);
        assert_eq!(unsafe { &*b_ptr }.fr_next, c_ptr);
        assert_eq!(unsafe { &*c_ptr }.fr_prev, b_ptr);
    }

    #[test]
    fn frame_append_at_the_end_of_a_list() {
        let mut a = crate::buffer_defs::FrameT::default();
        let a_ptr = &mut a as *mut crate::buffer_defs::FrameT;
        let mut b = crate::buffer_defs::FrameT::default();
        let b_ptr = &mut b as *mut crate::buffer_defs::FrameT;

        unsafe { frame_append(a_ptr, b_ptr) };

        assert_eq!(unsafe { &*a_ptr }.fr_next, b_ptr);
        assert_eq!(unsafe { &*b_ptr }.fr_prev, a_ptr);
        assert!(unsafe { &*b_ptr }.fr_next.is_null());
    }

    #[test]
    fn frame_insert_splices_into_the_middle_of_a_list() {
        let mut a = crate::buffer_defs::FrameT::default();
        let a_ptr = &mut a as *mut crate::buffer_defs::FrameT;
        let mut c =
            crate::buffer_defs::FrameT { fr_prev: a_ptr, ..Default::default() };
        let c_ptr = &mut c as *mut crate::buffer_defs::FrameT;
        unsafe { &mut *a_ptr }.fr_next = c_ptr;
        let mut b = crate::buffer_defs::FrameT::default();
        let b_ptr = &mut b as *mut crate::buffer_defs::FrameT;

        unsafe { frame_insert(c_ptr, b_ptr) };

        assert_eq!(unsafe { &*a_ptr }.fr_next, b_ptr);
        assert_eq!(unsafe { &*b_ptr }.fr_prev, a_ptr);
        assert_eq!(unsafe { &*b_ptr }.fr_next, c_ptr);
        assert_eq!(unsafe { &*c_ptr }.fr_prev, b_ptr);
    }

    #[test]
    fn frame_insert_at_the_start_of_a_list_updates_fr_parent_fr_child() {
        let mut parent = crate::buffer_defs::FrameT::default();
        let parent_ptr = &mut parent as *mut crate::buffer_defs::FrameT;
        let mut before = crate::buffer_defs::FrameT {
            fr_prev: std::ptr::null_mut(),
            fr_parent: parent_ptr,
            ..Default::default()
        };
        let before_ptr = &mut before as *mut crate::buffer_defs::FrameT;
        unsafe { &mut *parent_ptr }.fr_child = before_ptr;
        // frame_insert's own "no previous sibling" branch reads
        // frp->fr_parent (the NEW frame's own field, matching the
        // original exactly) - the caller must set this before
        // insertion whenever frp might end up as the new head.
        let mut frp = crate::buffer_defs::FrameT { fr_parent: parent_ptr, ..Default::default() };
        let frp_ptr = &mut frp as *mut crate::buffer_defs::FrameT;

        unsafe { frame_insert(before_ptr, frp_ptr) };

        assert_eq!(unsafe { &*parent_ptr }.fr_child, frp_ptr);
        assert_eq!(unsafe { &*frp_ptr }.fr_next, before_ptr);
        assert!(unsafe { &*frp_ptr }.fr_prev.is_null());
        assert_eq!(unsafe { &*before_ptr }.fr_prev, frp_ptr);
    }

    #[test]
    fn frame_remove_from_the_middle_relinks_both_neighbors() {
        let mut a = crate::buffer_defs::FrameT::default();
        let a_ptr = &mut a as *mut crate::buffer_defs::FrameT;
        let mut c = crate::buffer_defs::FrameT::default();
        let c_ptr = &mut c as *mut crate::buffer_defs::FrameT;
        let mut b = crate::buffer_defs::FrameT {
            fr_prev: a_ptr,
            fr_next: c_ptr,
            ..Default::default()
        };
        let b_ptr = &mut b as *mut crate::buffer_defs::FrameT;
        unsafe { &mut *a_ptr }.fr_next = b_ptr;
        unsafe { &mut *c_ptr }.fr_prev = b_ptr;

        unsafe { frame_remove(b_ptr) };

        assert_eq!(unsafe { &*a_ptr }.fr_next, c_ptr);
        assert_eq!(unsafe { &*c_ptr }.fr_prev, a_ptr);
    }

    #[test]
    fn frame_remove_the_first_node_updates_fr_parent_fr_child() {
        let mut parent = crate::buffer_defs::FrameT::default();
        let parent_ptr = &mut parent as *mut crate::buffer_defs::FrameT;
        let mut b = crate::buffer_defs::FrameT::default();
        let b_ptr = &mut b as *mut crate::buffer_defs::FrameT;
        let mut a = crate::buffer_defs::FrameT {
            fr_prev: std::ptr::null_mut(),
            fr_next: b_ptr,
            fr_parent: parent_ptr,
            ..Default::default()
        };
        let a_ptr = &mut a as *mut crate::buffer_defs::FrameT;
        unsafe { &mut *parent_ptr }.fr_child = a_ptr;
        unsafe { &mut *b_ptr }.fr_prev = a_ptr;

        unsafe { frame_remove(a_ptr) };

        assert_eq!(unsafe { &*parent_ptr }.fr_child, b_ptr);
        assert!(unsafe { &*b_ptr }.fr_prev.is_null());
    }

    #[test]
    fn frame_remove_the_last_node_leaves_prev_fr_next_null() {
        let mut a = crate::buffer_defs::FrameT::default();
        let a_ptr = &mut a as *mut crate::buffer_defs::FrameT;
        let mut b = crate::buffer_defs::FrameT {
            fr_prev: a_ptr,
            fr_next: std::ptr::null_mut(),
            ..Default::default()
        };
        let b_ptr = &mut b as *mut crate::buffer_defs::FrameT;
        unsafe { &mut *a_ptr }.fr_next = b_ptr;

        unsafe { frame_remove(b_ptr) };

        assert!(unsafe { &*a_ptr }.fr_next.is_null());
    }

    // ---- frame_has_win ----

    #[test]
    fn frame_has_win_leaf_matches_its_own_window() {
        let mut win = WinT { handle: 1, ..Default::default() };
        let win_ptr = &mut win as *mut WinT;
        let leaf = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_LEAF,
            fr_win: win_ptr,
            ..Default::default()
        };
        let leaf_ptr = &leaf as *const crate::buffer_defs::FrameT;

        assert!(unsafe { frame_has_win(leaf_ptr, win_ptr) });
    }

    #[test]
    fn frame_has_win_leaf_does_not_match_a_different_window() {
        let mut win = WinT { handle: 1, ..Default::default() };
        let win_ptr = &mut win as *mut WinT;
        let mut other = WinT { handle: 2, ..Default::default() };
        let other_ptr = &mut other as *mut WinT;
        let leaf = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_LEAF,
            fr_win: win_ptr,
            ..Default::default()
        };
        let leaf_ptr = &leaf as *const crate::buffer_defs::FrameT;

        assert!(!unsafe { frame_has_win(leaf_ptr, other_ptr) });
    }

    #[test]
    fn frame_has_win_finds_a_window_in_a_nested_child() {
        let mut win = WinT { handle: 1, ..Default::default() };
        let win_ptr = &mut win as *mut WinT;
        let mut leaf = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_LEAF,
            fr_win: win_ptr,
            ..Default::default()
        };
        let leaf_ptr = &mut leaf as *mut crate::buffer_defs::FrameT;
        let row = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_ROW,
            fr_child: leaf_ptr,
            ..Default::default()
        };
        let row_ptr = &row as *const crate::buffer_defs::FrameT;

        assert!(unsafe { frame_has_win(row_ptr, win_ptr) });
    }

    #[test]
    fn frame_has_win_false_when_no_child_matches() {
        let mut win = WinT { handle: 1, ..Default::default() };
        let win_ptr = &mut win as *mut WinT;
        let mut other = WinT { handle: 2, ..Default::default() };
        let other_ptr = &mut other as *mut WinT;
        let mut leaf = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_LEAF,
            fr_win: other_ptr,
            ..Default::default()
        };
        let leaf_ptr = &mut leaf as *mut crate::buffer_defs::FrameT;
        let row = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_ROW,
            fr_child: leaf_ptr,
            ..Default::default()
        };
        let row_ptr = &row as *const crate::buffer_defs::FrameT;

        assert!(!unsafe { frame_has_win(row_ptr, win_ptr) });
    }

    // ---- set_fraction ----

    #[test]
    fn set_fraction_computes_the_relative_row() {
        let mut win = WinT { w_wrow: 5, w_view_height: 20, ..Default::default() };
        set_fraction(&mut win);
        assert_eq!(win.w_fraction, 4505);
    }

    #[test]
    fn set_fraction_no_op_when_view_height_is_one_or_less() {
        let mut win = WinT { w_wrow: 5, w_view_height: 1, w_fraction: 999, ..Default::default() };
        set_fraction(&mut win);
        assert_eq!(win.w_fraction, 999);

        win.w_view_height = 0;
        set_fraction(&mut win);
        assert_eq!(win.w_fraction, 999);
    }

    // ---- win_altframe ----

    /// RAII guard for `win_altframe` tests: saves/restores every
    /// `GLOBALS`/`OPTION_VARS` field it reads.
    struct WinAltframeGuard {
        prev_first_tabpage: *mut crate::buffer_defs::TabpageT,
        prev_curtab: *mut crate::buffer_defs::TabpageT,
        prev_firstwin: *mut WinT,
        prev_lastused_tabpage: *mut crate::buffer_defs::TabpageT,
        prev_tcl_flags: u32,
        prev_p_sb: i32,
        prev_p_spr: i32,
    }
    impl WinAltframeGuard {
        fn set(
            first_tabpage: *mut crate::buffer_defs::TabpageT,
            curtab: *mut crate::buffer_defs::TabpageT,
            firstwin: *mut WinT,
            p_sb: i32,
            p_spr: i32,
        ) -> Self {
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            let guard = WinAltframeGuard {
                prev_first_tabpage: g.first_tabpage,
                prev_curtab: g.curtab,
                prev_firstwin: g.firstwin,
                prev_lastused_tabpage: g.lastused_tabpage,
                prev_tcl_flags: opts.tcl_flags,
                prev_p_sb: opts.p_sb,
                prev_p_spr: opts.p_spr,
            };
            g.first_tabpage = first_tabpage;
            g.curtab = curtab;
            g.firstwin = firstwin;
            g.lastused_tabpage = std::ptr::null_mut();
            opts.tcl_flags = 0;
            opts.p_sb = p_sb;
            opts.p_spr = p_spr;
            guard
        }
    }
    impl Drop for WinAltframeGuard {
        fn drop(&mut self) {
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            g.first_tabpage = self.prev_first_tabpage;
            g.curtab = self.prev_curtab;
            g.firstwin = self.prev_firstwin;
            g.lastused_tabpage = self.prev_lastused_tabpage;
            opts.tcl_flags = self.prev_tcl_flags;
            opts.p_sb = self.prev_p_sb;
            opts.p_spr = self.prev_p_spr;
        }
    }

    #[test]
    fn win_altframe_one_window_uses_alt_tabpage_curwin_frame() {
        let _lock = crate::globals::global_state_test_lock();
        let mut alt_frame = crate::buffer_defs::FrameT::default();
        let alt_frame_ptr = &mut alt_frame as *mut crate::buffer_defs::FrameT;
        let mut alt_win = WinT { w_frame: alt_frame_ptr, ..Default::default() };
        let alt_win_ptr = &mut alt_win as *mut WinT;
        let mut tp2 = crate::buffer_defs::TabpageT { tp_curwin: alt_win_ptr, ..Default::default() };
        let tp2_ptr = &mut tp2 as *mut crate::buffer_defs::TabpageT;
        let mut win = WinT { handle: 1, w_next: std::ptr::null_mut(), ..Default::default() };
        let win_ptr = &mut win as *mut WinT;
        let mut tp1 =
            crate::buffer_defs::TabpageT { tp_next: tp2_ptr, tp_firstwin: win_ptr, ..Default::default() };
        let tp1_ptr = &mut tp1 as *mut crate::buffer_defs::TabpageT;
        let _guard = WinAltframeGuard::set(tp1_ptr, tp1_ptr, win_ptr, 0, 0);

        // win is the ONLY window (firstwin == win, w_next null) - alt
        // tabpage (tp2, since curtab.tp_next is non-null and no LEFT
        // flag) provides the result via its own tp_curwin.w_frame.
        assert_eq!(unsafe { win_altframe(win_ptr, std::ptr::null()) }, alt_frame_ptr);
    }

    #[test]
    fn win_altframe_no_prev_returns_next() {
        let _lock = crate::globals::global_state_test_lock();
        let mut next = crate::buffer_defs::FrameT::default();
        let next_ptr = &mut next as *mut crate::buffer_defs::FrameT;
        let mut frame = crate::buffer_defs::FrameT {
            fr_prev: std::ptr::null_mut(),
            fr_next: next_ptr,
            ..Default::default()
        };
        // A second window keeps one_window() false.
        let mut second = WinT { handle: 2, ..Default::default() };
        let second_ptr = &mut second as *mut WinT;
        let mut win = WinT { handle: 1, w_frame: &mut frame as *mut _, w_next: second_ptr, ..Default::default() };
        let win_ptr = &mut win as *mut WinT;
        let mut tp = crate::buffer_defs::TabpageT { tp_firstwin: win_ptr, ..Default::default() };
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = WinAltframeGuard::set(tp_ptr, tp_ptr, win_ptr, 0, 0);

        assert_eq!(unsafe { win_altframe(win_ptr, std::ptr::null()) }, next_ptr);
    }

    #[test]
    fn win_altframe_no_next_returns_prev() {
        let _lock = crate::globals::global_state_test_lock();
        let mut prev = crate::buffer_defs::FrameT::default();
        let prev_ptr = &mut prev as *mut crate::buffer_defs::FrameT;
        let mut frame = crate::buffer_defs::FrameT {
            fr_prev: prev_ptr,
            fr_next: std::ptr::null_mut(),
            ..Default::default()
        };
        let mut second = WinT { handle: 2, ..Default::default() };
        let second_ptr = &mut second as *mut WinT;
        let mut win = WinT { handle: 1, w_frame: &mut frame as *mut _, w_next: second_ptr, ..Default::default() };
        let win_ptr = &mut win as *mut WinT;
        let mut tp = crate::buffer_defs::TabpageT { tp_firstwin: win_ptr, ..Default::default() };
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = WinAltframeGuard::set(tp_ptr, tp_ptr, win_ptr, 0, 0);

        assert_eq!(unsafe { win_altframe(win_ptr, std::ptr::null()) }, prev_ptr);
    }

    #[test]
    fn win_altframe_default_uses_next_frame() {
        let _lock = crate::globals::global_state_test_lock();
        let mut prev = crate::buffer_defs::FrameT::default();
        let prev_ptr = &mut prev as *mut crate::buffer_defs::FrameT;
        let mut next = crate::buffer_defs::FrameT::default();
        let next_ptr = &mut next as *mut crate::buffer_defs::FrameT;
        // No parent at all - the splitbelow/splitright/reversal
        // checks are all skipped (fr_parent.is_null()).
        let mut frame = crate::buffer_defs::FrameT {
            fr_prev: prev_ptr,
            fr_next: next_ptr,
            fr_parent: std::ptr::null_mut(),
            ..Default::default()
        };
        let mut second = WinT { handle: 2, ..Default::default() };
        let second_ptr = &mut second as *mut WinT;
        let mut win = WinT { handle: 1, w_frame: &mut frame as *mut _, w_next: second_ptr, ..Default::default() };
        let win_ptr = &mut win as *mut WinT;
        let mut tp = crate::buffer_defs::TabpageT { tp_firstwin: win_ptr, ..Default::default() };
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = WinAltframeGuard::set(tp_ptr, tp_ptr, win_ptr, 0, 0);

        assert_eq!(unsafe { win_altframe(win_ptr, std::ptr::null()) }, next_ptr);
    }

    #[test]
    fn win_altframe_splitbelow_in_a_column_uses_prev_frame() {
        let _lock = crate::globals::global_state_test_lock();
        let mut parent = crate::buffer_defs::FrameT { fr_layout: crate::buffer_defs::FR_COL, ..Default::default() };
        let parent_ptr = &mut parent as *mut crate::buffer_defs::FrameT;
        let mut prev = crate::buffer_defs::FrameT::default();
        let prev_ptr = &mut prev as *mut crate::buffer_defs::FrameT;
        let mut next = crate::buffer_defs::FrameT::default();
        let next_ptr = &mut next as *mut crate::buffer_defs::FrameT;
        let mut frame = crate::buffer_defs::FrameT {
            fr_prev: prev_ptr,
            fr_next: next_ptr,
            fr_parent: parent_ptr,
            ..Default::default()
        };
        let mut second = WinT { handle: 2, ..Default::default() };
        let second_ptr = &mut second as *mut WinT;
        let mut win = WinT { handle: 1, w_frame: &mut frame as *mut _, w_next: second_ptr, ..Default::default() };
        let win_ptr = &mut win as *mut WinT;
        let mut tp = crate::buffer_defs::TabpageT { tp_firstwin: win_ptr, ..Default::default() };
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        // 'splitbelow' is set (p_sb=1).
        let _guard = WinAltframeGuard::set(tp_ptr, tp_ptr, win_ptr, 1, 0);

        assert_eq!(unsafe { win_altframe(win_ptr, std::ptr::null()) }, prev_ptr);
    }

    #[test]
    fn win_altframe_reverses_when_target_has_fixed_width_and_other_does_not() {
        let _lock = crate::globals::global_state_test_lock();
        let mut parent = crate::buffer_defs::FrameT { fr_layout: crate::buffer_defs::FR_ROW, ..Default::default() };
        let parent_ptr = &mut parent as *mut crate::buffer_defs::FrameT;

        // fr_next (the default target) is a LEAF with 'winfixwidth'
        // set; fr_prev (other) is a LEAF without it.
        let mut fixed_win = WinT {
            handle: 3,
            w_onebuf_opt: crate::buffer_defs::WinoptT { wo_wfw: 1, ..Default::default() },
            ..Default::default()
        };
        let fixed_win_ptr = &mut fixed_win as *mut WinT;
        let mut next_frame = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_LEAF,
            fr_win: fixed_win_ptr,
            ..Default::default()
        };
        let next_ptr = &mut next_frame as *mut crate::buffer_defs::FrameT;

        let mut movable_win = WinT { handle: 4, ..Default::default() };
        let movable_win_ptr = &mut movable_win as *mut WinT;
        let mut prev_frame = crate::buffer_defs::FrameT {
            fr_layout: crate::buffer_defs::FR_LEAF,
            fr_win: movable_win_ptr,
            ..Default::default()
        };
        let prev_ptr = &mut prev_frame as *mut crate::buffer_defs::FrameT;

        let mut frame = crate::buffer_defs::FrameT {
            fr_prev: prev_ptr,
            fr_next: next_ptr,
            fr_parent: parent_ptr,
            ..Default::default()
        };
        let mut second = WinT { handle: 2, ..Default::default() };
        let second_ptr = &mut second as *mut WinT;
        let mut win = WinT { handle: 1, w_frame: &mut frame as *mut _, w_next: second_ptr, ..Default::default() };
        let win_ptr = &mut win as *mut WinT;
        let mut tp = crate::buffer_defs::TabpageT { tp_firstwin: win_ptr, ..Default::default() };
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let _guard = WinAltframeGuard::set(tp_ptr, tp_ptr, win_ptr, 0, 0);

        // Default target is fr_next (fixed width) - reversed to
        // fr_prev since it is NOT fixed width.
        assert_eq!(unsafe { win_altframe(win_ptr, std::ptr::null()) }, prev_ptr);
    }

    // ---- cmd_with_count ----

    #[test]
    fn cmd_with_count_appends_a_positive_count() {
        assert_eq!(cmd_with_count(b"quit", 3), b"quit3".to_vec());
    }

    #[test]
    fn cmd_with_count_zero_leaves_cmd_unchanged() {
        assert_eq!(cmd_with_count(b"quit", 0), b"quit".to_vec());
    }

    #[test]
    fn cmd_with_count_negative_leaves_cmd_unchanged() {
        assert_eq!(cmd_with_count(b"quit", -1), b"quit".to_vec());
    }

    #[test]
    fn cmd_with_count_multi_digit_count() {
        assert_eq!(cmd_with_count(b"split", 42), b"split42".to_vec());
    }

    // ---- win_find_tabpage ----

    #[test]
    fn win_find_tabpage_finds_a_window_in_curtab() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT { handle: 1, ..Default::default() };
        let win_ptr = &mut win as *mut WinT;
        let mut tp = crate::buffer_defs::TabpageT::default();
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let (prev_first_tabpage, prev_curtab, prev_firstwin) = (g.first_tabpage, g.curtab, g.firstwin);
        g.first_tabpage = tp_ptr;
        g.curtab = tp_ptr;
        g.firstwin = win_ptr;

        assert_eq!(unsafe { win_find_tabpage(win_ptr) }, tp_ptr);

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.first_tabpage = prev_first_tabpage;
        g.curtab = prev_curtab;
        g.firstwin = prev_firstwin;
    }

    #[test]
    fn win_find_tabpage_finds_a_window_in_a_non_curtab_tabpage() {
        let _lock = crate::globals::global_state_test_lock();
        let mut other_win = WinT { handle: 2, ..Default::default() };
        let other_win_ptr = &mut other_win as *mut WinT;
        let mut other_tp =
            crate::buffer_defs::TabpageT { tp_firstwin: other_win_ptr, ..Default::default() };
        let other_tp_ptr = &mut other_tp as *mut crate::buffer_defs::TabpageT;

        let mut cur_win = WinT { handle: 1, ..Default::default() };
        let cur_win_ptr = &mut cur_win as *mut WinT;
        let mut cur_tp =
            crate::buffer_defs::TabpageT { tp_next: other_tp_ptr, ..Default::default() };
        let cur_tp_ptr = &mut cur_tp as *mut crate::buffer_defs::TabpageT;

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let (prev_first_tabpage, prev_curtab, prev_firstwin) = (g.first_tabpage, g.curtab, g.firstwin);
        g.first_tabpage = cur_tp_ptr;
        g.curtab = cur_tp_ptr;
        g.firstwin = cur_win_ptr;

        assert_eq!(unsafe { win_find_tabpage(other_win_ptr) }, other_tp_ptr);

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.first_tabpage = prev_first_tabpage;
        g.curtab = prev_curtab;
        g.firstwin = prev_firstwin;
    }

    #[test]
    fn win_find_tabpage_null_when_not_found() {
        let _lock = crate::globals::global_state_test_lock();
        let mut listed = WinT { handle: 1, ..Default::default() };
        let listed_ptr = &mut listed as *mut WinT;
        let not_listed = WinT { handle: 2, ..Default::default() };
        let mut tp = crate::buffer_defs::TabpageT::default();
        let tp_ptr = &mut tp as *mut crate::buffer_defs::TabpageT;

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let (prev_first_tabpage, prev_curtab, prev_firstwin) = (g.first_tabpage, g.curtab, g.firstwin);
        g.first_tabpage = tp_ptr;
        g.curtab = tp_ptr;
        g.firstwin = listed_ptr;

        assert!(unsafe { win_find_tabpage(&not_listed as *const WinT) }.is_null());

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.first_tabpage = prev_first_tabpage;
        g.curtab = prev_curtab;
        g.firstwin = prev_firstwin;
    }
}
