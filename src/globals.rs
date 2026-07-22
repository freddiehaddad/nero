//! Translated from `src/nvim/globals.h`.
//!
//! The original is a single, flat header of hundreds of `EXTERN`
//! (roughly: "declare, and on exactly one translation unit, define with
//! this initial value") global mutable variables covering nearly every
//! subsystem of the editor - current window/buffer/tabpage, message
//! state, exception handling, mouse state, ex-command nesting, debug
//! state, and so on. This is a deliberate, real architectural feature of
//! the original (every function can reach "the current window" or "the
//! current buffer" or "is a message being displayed right now" via a
//! plain global), not an incidental detail to redesign away.
//!
//! Translated as a single [`Globals`] struct whose fields mirror every
//! `EXTERN` declaration, in the same order, grouped with the same
//! section comments as the original - a literal transliteration rather
//! than a split into per-subsystem globals, so that this remains
//! directly diffable against `globals.h` and the "single source of
//! global mutable editor state" architecture is preserved for whichever
//! future file (`main.c`/`state.c`) actually creates and owns the one
//! live instance. Field names keep their exact original spelling
//! (including the original's own non-`snake_case` names like `Rows`,
//! `State`, `Ins`, `Visual`, `Search`) rather than being renamed to fit
//! Rust conventions, since a field name is being transliterated (not a
//! type name, which does get PascalCase per this crate's convention) -
//! `#[allow(non_snake_case)]` silences the resulting lint.
//!
//! Deferred (fields whose type is itself not yet translatable):
//! `scriptvar_T`/`dict_T`-by-value fields don't appear directly in this
//! header, but `tabpage_T`/`ufunc_T`/`AutoPatCmd`/`regmatch_T`/`Loop`
//! pointers use this crate's existing opaque placeholders (see
//! `types_defs.rs`) exactly as the original only ever references them
//! through a pointer here too - nothing in `globals.h` itself embeds one
//! of those by value, so no field had to be dropped.
//!
//! Every one of the original's 248 `EXTERN` declarations (246 plain
//! variables plus the two anonymous-struct instances `g_stats`/
//! `provider_caller_scope`) is present here, cross-checked by
//! mechanically diffing an extracted field-name list from the real
//! 727-line `globals.h` against this struct's field list (a first pass
//! mistakenly used a truncated 585-line read of the file - fixed and
//! reconfirmed against the true, full file before this was considered
//! complete).

use std::cell::UnsafeCell;

use crate::arglist_defs::AlistT;
use crate::buffer_defs::{BufT, DisptickT, FrameT, TabpageT, WinT};
use crate::eval::typval_defs::SctxT;
use crate::ex_cmds_defs::CmdmodT;
use crate::ex_eval_defs::{ExceptT, MsglistT};
use crate::garray_defs::GarrayT;
use crate::gettext_defs::gettext_noop;
use crate::highlight_defs::HlfT;
use crate::input_defs::TypebufT;
use crate::insert_defs::InsState;
use crate::menu_defs::VimMenu;
use crate::normal_defs::VisualState;
use crate::os::os_defs::MAXPATHL;
use crate::pos_defs::{ColnrT, LinenrT, PosT, MAXLNUM};
use crate::regexp_defs::OptmagicT;
use crate::runtime_defs::EstackT;
use crate::search_defs::SearchState;
use crate::state_defs::mode;
use crate::types_defs::{RegExtmatchT, TriState};

/// file I/O and sprintf buffer size (`IOSIZE`).
pub const IOSIZE: usize = 1024 + 1;

/// length of buffer for small messages (`MSG_BUF_LEN`).
pub const MSG_BUF_LEN: usize = 480;
/// cell length (worst case: utf-8 takes 6 bytes for one cell)
/// (`MSG_BUF_CLEN`).
pub const MSG_BUF_CLEN: usize = MSG_BUF_LEN / 6;

// FILETYPE_FILE        used for file type detection
// FTPLUGIN_FILE        used for loading filetype plugin files
// INDENT_FILE          used for loading indent files
// FTOFF_FILE           used for file type detection
// FTPLUGOF_FILE        used for loading settings files
// INDOFF_FILE          used for loading indent files
//
// These are all `#ifndef X #define X "..."` in the original (so a build
// system could override them via a compile define); translated as the
// default string value only, since no build-flag-override mechanism
// exists in this crate yet and nothing consumes these until runtime.c
// (not yet translated) is reached.
pub const FILETYPE_FILE: &str = "filetype.lua filetype.vim";
pub const FTPLUGIN_FILE: &str = "ftplugin.vim";
pub const INDENT_FILE: &str = "indent.vim";
pub const FTOFF_FILE: &str = "ftoff.vim";
pub const FTPLUGOF_FILE: &str = "ftplugof.vim";
pub const INDOFF_FILE: &str = "indoff.vim";
pub const DFLT_ERRORFILE: &str = "errors.err";
pub const SYS_VIMRC_FILE: &str = "$VIM/sysinit.vim";
pub const DFLT_HELPFILE: &str = "$VIMRUNTIME/doc/help.txt";
pub const SYNTAX_FNAME: &str = "$VIMRUNTIME/syntax/%s.vim";
pub const EXRC_FILE: &str = ".exrc";
pub const VIMRC_FILE: &str = ".nvimrc";
pub const VIMRC_LUA_FILE: &str = ".nvim.lua";

/// Statistics/counters (`struct nvim_stats_s`, instantiated as the
/// global `g_stats`).
#[derive(Debug, Clone, Copy, Default)]
pub struct NvimStatsS {
    pub fsync: i64,
    pub redraw: i64,
    /// How many logs were tried and skipped before `log_init`.
    pub log_skip: i16,
}

// Values for "starting".
/// no screen updating yet (`NO_SCREEN`).
pub const NO_SCREEN: i32 = 2;
/// not all buffers loaded yet (`NO_BUFFERS`).
pub const NO_BUFFERS: i32 = 1;
//                      0          not starting anymore

/// default value for `'columns'` (`DFLT_COLS`).
pub const DFLT_COLS: i32 = 80;
/// default value for `'lines'` (`DFLT_ROWS`).
pub const DFLT_ROWS: i32 = 24;

// Special values for current_SID.
pub const SID_MODELINE: i32 = -1;
pub const SID_CMDARG: i32 = -2;
pub const SID_CARG: i32 = -3;
pub const SID_ENV: i32 = -4;
pub const SID_ERROR: i32 = -5;
pub const SID_NONE: i32 = -6;
pub const SID_WINLAYOUT: i32 = -7;
pub const SID_LUA: i32 = -8;
pub const SID_API_CLIENT: i32 = -9;
pub const SID_STR: i32 = -10;

// These flags are set based upon 'fileencoding'. The characters are
// internally stored as UTF-8 to avoid trouble with NUL bytes.
pub const DBCS_JPN: i32 = 932;
pub const DBCS_JPNU: i32 = 9932;
pub const DBCS_KOR: i32 = 949;
pub const DBCS_KORU: i32 = 9949;
pub const DBCS_CHS: i32 = 936;
pub const DBCS_CHSU: i32 = 9936;
pub const DBCS_CHT: i32 = 950;
pub const DBCS_CHTU: i32 = 9950;
pub const DBCS_2BYTE: i32 = 1;
pub const DBCS_DEBUG: i32 = -1;

// Values for swap_exists_action: what to do when swap file already
// exists.
/// don't use dialog (`SEA_NONE`).
pub const SEA_NONE: i32 = 0;
/// use dialog when possible (`SEA_DIALOG`).
pub const SEA_DIALOG: i32 = 1;
/// quit editing the file (`SEA_QUIT`).
pub const SEA_QUIT: i32 = 2;
/// recover the file (`SEA_RECOVER`).
pub const SEA_RECOVER: i32 = 3;
/// no dialog, mark buffer as read-only (`SEA_READONLY`).
pub const SEA_READONLY: i32 = 4;

/// Scope information for the code that indirectly triggered the current
/// provider function call (`struct caller_scope`, instantiated as the
/// global `provider_caller_scope`).
pub struct CallerScope {
    pub script_ctx: SctxT,
    pub es_entry: EstackT,
    pub autocmd_fname: Option<Vec<u8>>,
    pub autocmd_match: Option<Vec<u8>>,
    pub autocmd_fname_full: bool,
    pub autocmd_bufnr: i32,
    pub funccalp: *mut std::ffi::c_void,
}

impl Default for CallerScope {
    fn default() -> Self {
        CallerScope {
            script_ctx: SctxT::default(),
            es_entry: EstackT::default(),
            autocmd_fname: None,
            autocmd_match: None,
            autocmd_fname_full: false,
            autocmd_bufnr: 0,
            funccalp: std::ptr::null_mut(),
        }
    }
}

/// max mode length returned in `get_mode()`, including the terminating
/// NUL (`MODE_MAX_LENGTH`).
pub const MODE_MAX_LENGTH: usize = 4;

/// wildmenu showing (`WM_SHOWN`).
pub const WM_SHOWN: i32 = 1;
/// wildmenu showing with scroll (`WM_SCROLLED`).
pub const WM_SCROLLED: i32 = 2;

// Whether titlestring and iconstring contains statusline syntax.
pub const STL_IN_ICON: i32 = 1;
pub const STL_IN_TITLE: i32 = 2;

/// Size of `os_buf`: `MAXPATHL` if bigger than [`IOSIZE`], else
/// [`IOSIZE`] (matches the original's `#if MAXPATHL > IOSIZE` choice).
pub const OS_BUF_SIZE: usize = if (MAXPATHL as usize) > IOSIZE { MAXPATHL as usize } else { IOSIZE };

/// Every `EXTERN` global variable declared in `globals.h`, as one struct
/// (see the module-level doc comment for why this isn't split up).
#[allow(non_snake_case)]
pub struct Globals {
    pub g_stats: NvimStatsS,

    /// nr of rows in the screen
    pub Rows: i32,
    /// nr of columns in the screen
    pub Columns: i32,

    /// current key modifiers
    pub mod_mask: i32,
    pub vgetc_mod_mask: i32,
    pub vgetc_char: i32,

    /// Cmdline_row is the row where the command line starts, just below
    /// the last window.
    pub cmdline_row: i32,

    /// cmdline must be redrawn
    pub redraw_cmdline: bool,
    /// mode must be redrawn
    pub redraw_mode: bool,
    /// cmdline must be cleared
    pub clear_cmdline: bool,
    /// mode is being displayed
    pub mode_displayed: bool,
    /// cmdline is encrypted
    pub cmdline_star: i32,
    /// cmdline is being redrawn
    pub redrawing_cmdline: bool,
    /// cmdline was last drawn
    pub cmdline_was_last_drawn: bool,

    /// executing register
    pub exec_from_reg: bool,

    /// virtual column of a displayed `'$'` marking a partial change
    /// (`'cpoptions'` `'$'` flag); -1 means no `$` is being displayed.
    pub dollar_vcol: ColnrT,

    // Variables for Insert mode completion.
    /// msg for CTRL-X submode
    pub edit_submode: Option<Vec<u8>>,
    /// prepended to `edit_submode`
    pub edit_submode_pre: Option<Vec<u8>>,
    /// appended to `edit_submode`
    pub edit_submode_extra: Option<Vec<u8>>,
    /// highl. method for extra info
    pub edit_submode_highl: HlfT,

    // State for putting characters in the message area.
    /// cmdline is drawn right to left
    pub cmdmsg_rl: bool,
    pub msg_col: i32,
    pub msg_row: i32,
    /// Number of screen lines that messages have scrolled.
    pub msg_scrolled: i32,
    /// when true don't set `need_wait_return` in `msg_puts_attr()` when
    /// `msg_scrolled` is non-zero
    pub msg_scrolled_ign: bool,
    /// Whether the screen is damaged due to scrolling. Sometimes
    /// `msg_scrolled` is reset before the screen is redrawn, so we need
    /// to keep track of this.
    pub msg_did_scroll: bool,

    /// msg to be shown after redraw
    pub keep_msg: Option<Vec<u8>>,
    /// highlight id for `keep_msg`
    pub keep_msg_hl_id: i32,
    /// do fileinfo() after redraw
    pub need_fileinfo: bool,
    /// `msg_start()` will scroll
    pub msg_scroll: i32,
    /// `msg_outstr()` was used in line
    pub msg_didout: bool,
    /// `msg_outstr()` was used at all
    pub msg_didany: bool,
    /// don't wait for this msg
    pub msg_nowait: bool,
    /// don't display errors for now, unless `'debug'` is set.
    pub emsg_off: i32,
    /// printing informative message
    pub info_message: bool,
    /// don't add messages to history
    pub msg_hist_off: bool,
    /// need to clear text before displaying a message.
    pub need_clr_eos: bool,
    /// don't display errors for expression that is skipped
    pub emsg_skip: i32,
    /// use message of next of several `emsg()` calls for throw
    pub emsg_severe: bool,
    // used by assert_fails()
    pub emsg_assert_fails_msg: Option<Vec<u8>>,
    pub emsg_assert_fails_lnum: i64,
    pub emsg_assert_fails_context: Option<Vec<u8>>,

    /// just had `":endif"`
    pub did_endif: bool,
    /// incremented by `emsg()` when a message is displayed or thrown
    pub did_emsg: i32,
    /// set if `vim_beep()` is called
    pub called_vim_beep: bool,
    /// `did_emsg` set because of a syntax error
    pub did_emsg_syntax: bool,
    /// always incremented by `emsg()`
    pub called_emsg: i32,
    /// exit value for ex mode
    pub ex_exitval: i32,
    /// there is an error message
    pub emsg_on_display: bool,
    /// `vim_regcomp()` called `emsg()`
    pub rc_did_emsg: bool,

    /// don't wait for return for now
    pub no_wait_return: i32,
    /// need to wait for return later
    pub need_wait_return: bool,
    /// `wait_return()` was used and nothing written since then
    pub did_wait_return: bool,
    /// call `maketitle()` soon
    pub need_maketitle: bool,

    /// 'q' hit at "--more--" msg
    pub quit_more: bool,
    /// when inside `vgetc()` then > 0
    pub vgetc_busy: i32,

    /// did set $VIM ourselves
    pub didset_vim: bool,
    /// idem for $VIMRUNTIME
    pub didset_vimruntime: bool,

    /// lines left for listing. Ex mode needs to be able to reset this
    /// after you type something.
    pub lines_left: i32,
    /// don't use more prompt, truncate messages
    pub msg_no_more: bool,

    /// nesting level
    pub ex_nesting_level: i32,
    /// break below this level
    pub debug_break_level: i32,
    /// did "debug mode" message
    pub debug_did_msg: bool,
    /// breakpoint change count
    pub debug_tick: i32,
    /// breakpoint backtrace level
    pub debug_backtrace_level: i32,

    /// `PROF_*` values
    pub do_profiling: i32,

    /// Exception currently being thrown. Used to pass an exception to a
    /// different cstack. Also used for discarding an exception before
    /// it is caught or made pending. Only valid when `did_throw` is
    /// true.
    pub current_exception: *mut ExceptT,

    /// An exception is being thrown. Reset when the exception is caught
    /// or as long as it is pending in a finally clause.
    pub did_throw: bool,

    /// Set when a throw that cannot be handled in `do_cmdline()` must be
    /// propagated to the cstack of the previously called
    /// `do_cmdline()`.
    pub need_rethrow: bool,

    /// Set when a `":finish"` or `":return"` that cannot be handled in
    /// `do_cmdline()` must be propagated to the cstack of the previously
    /// called `do_cmdline()`.
    pub check_cstack: bool,

    /// Number of nested try conditionals (across function calls and
    /// `":source"` commands).
    pub trylevel: i32,

    /// When true, always skip commands after an error message, even
    /// after the outermost `":endif"`, `":endwhile"` or `":endfor"` or
    /// for a function without the "abort" flag. Set to true when
    /// `trylevel` is non-zero (and `":silent!"` was not used) or an
    /// exception is being thrown at the time an error is detected. Set
    /// to false when `trylevel` gets zero again and there was no error
    /// or interrupt or throw.
    pub force_abort: bool,

    /// Points to a variable in the stack of `do_cmdline()` which keeps
    /// the list of arguments of several `emsg()` calls, one of which is
    /// to be converted to an error exception immediately after the
    /// failing command returns. The message to be used for the
    /// exception value is pointed to by the `"throw_msg"` field of the
    /// first element in the list. It is usually the same as the "msg"
    /// field of that element, but can be identical to the "msg" field of
    /// a later list element, when the `emsg_severe` flag was set when
    /// the `emsg()` call was made.
    pub msg_list: *mut *mut MsglistT,

    /// When set, don't convert an error to an exception. Used when
    /// displaying the interrupt message or reporting an exception that
    /// is still uncaught at the top level (which has already been
    /// discarded then). Also used for the error message when no
    /// exception can be thrown.
    pub suppress_errthrow: bool,

    /// The stack of all caught and not finished exceptions. The
    /// exception on the top of the stack is the one got by evaluation of
    /// `v:exception`. The complete stack of all caught and pending
    /// exceptions is embedded in the various cstacks; the pending
    /// exceptions, however, are not on the caught stack.
    pub caught_stack: *mut ExceptT,

    // Garbage collection can only take place when we are sure there are
    // no Lists or Dictionaries being used internally. This is flagged
    // with "may_garbage_collect" when we are at the toplevel.
    // "want_garbage_collect" is set by the garbagecollect() function,
    // which means we do garbage collection before waiting for a char at
    // the toplevel. "garbage_collect_at_exit" indicates
    // garbagecollect(1) was called.
    pub may_garbage_collect: bool,
    pub want_garbage_collect: bool,
    pub garbage_collect_at_exit: bool,

    /// Script CTX being sourced or was sourced to define the current
    /// function.
    pub current_sctx: SctxT,
    /// Last channel that invoked `nvim_input` or got FocusGained.
    pub current_ui: u64,

    pub did_source_packages: bool,

    // Scope information for the code that indirectly triggered the
    // current provider function call.
    pub provider_caller_scope: CallerScope,
    pub provider_call_nesting: i32,

    /// int value of `T_CCO`
    pub t_colors: i32,

    // Flags to indicate an additional string for highlight name
    // completion.
    /// when 1 include "None"
    pub include_none: i32,
    /// when 1 include "default"
    pub include_default: i32,
    /// when 2 include "link" and "clear"
    pub include_link: i32,

    /// Per-subsystem state for the search/highlight engine; see
    /// `search_defs.h`.
    pub Search: SearchState,

    /// need to check file timestamps asap
    pub need_check_timestamps: bool,
    /// did check timestamps recently
    pub did_check_timestamps: bool,
    /// Don't check timestamps
    pub no_check_timestamps: i32,

    // Mouse coordinates, set by handle_mouse_event().
    pub mouse_grid: i32,
    pub mouse_row: i32,
    pub mouse_col: i32,
    /// mouse below last line
    pub mouse_past_bottom: bool,
    /// mouse right of line
    pub mouse_past_eol: bool,
    /// extending Visual area with mouse dragging
    pub mouse_dragging: i32,

    /// The root of the menu hierarchy.
    pub root_menu: *mut VimMenu,
    /// While defining the system menu, `sys_menu` is true. This avoids
    /// overruling of menus that the user already defined.
    pub sys_menu: bool,

    // All windows are linked in a list. firstwin points to the first
    // entry, lastwin to the last entry (can be the same as firstwin) and
    // curwin to the currently active window.
    /// first window
    pub firstwin: *mut WinT,
    /// last window
    pub lastwin: *mut WinT,
    /// previous window (may equal curwin)
    pub prevwin: *mut WinT,

    /// currently active window
    pub curwin: *mut WinT,

    /// The window layout is kept in a tree of frames. `topframe` points
    /// to the top of the tree.
    pub topframe: *mut FrameT,

    // Tab pages are alternative topframes. "first_tabpage" points to the
    // first one in the list, "curtab" is the current one.
    // "lastused_tabpage" is the last used one.
    pub first_tabpage: *mut TabpageT,
    pub curtab: *mut TabpageT,
    pub lastused_tabpage: *mut TabpageT,
    /// need to redraw tabline
    pub redraw_tabline: bool,

    // All buffers are linked in a list. 'firstbuf' points to the first
    // entry, 'lastbuf' to the last entry and 'curbuf' to the currently
    // active buffer.
    /// first buffer
    pub firstbuf: *mut BufT,
    /// last buffer
    pub lastbuf: *mut BufT,
    /// currently active buffer
    pub curbuf: *mut BufT,

    /// global argument list
    pub global_alist: AlistT,
    /// the previous argument list id
    pub max_alist_id: i32,
    /// accessed last file in `global_alist`
    pub arg_had_last: bool,

    /// column for ruler
    pub ru_col: i32,
    /// `'rulerfmt'` width of ruler when non-zero
    pub ru_wid: i32,
    /// column for shown command
    pub sc_col: i32,

    // When starting or exiting some things are done differently (e.g.
    // screen updating). First NO_SCREEN, then NO_BUFFERS, then 0 when
    // startup finished.
    pub starting: i32,
    /// Planning to exit. Might keep running if there is a changed
    /// buffer.
    pub exiting: bool,
    /// Internal value of `v:dying`
    pub v_dying: i32,
    /// Is stdin a terminal?
    pub stdin_isatty: bool,
    /// Is stdout a terminal?
    pub stdout_isatty: bool,
    /// Is stderr a terminal?
    pub stderr_isatty: bool,

    /// Filedesc set by embedder for reading first buffer like `cmd |
    /// nvim -`.
    pub stdin_fd: i32,

    /// true when doing full-screen output, otherwise only writing some
    /// messages.
    pub full_screen: bool,

    /// Non-zero when only "safe" commands are allowed
    pub secure: i32,

    /// Non-zero when changing text and jumping to another window or
    /// editing another buffer is not allowed.
    pub textlock: i32,

    /// Non-zero when no buffer name can be changed, no buffer can be
    /// deleted and current directory can't be changed. Used for
    /// `SwapExists` et al.
    pub allbuf_lock: i32,

    /// Non-zero when evaluating an expression in a "sandbox". Several
    /// things are not allowed then.
    pub sandbox: i32,

    /// Batch-mode: "-es", "-Es", "-l" commandline argument was given.
    pub silent_mode: bool,

    /// Per-subsystem state for Visual/Select mode; see `normal_defs.h`.
    pub Visual: VisualState,

    /// When pasting text with the middle mouse button in visual mode
    /// with `restart_edit` set, remember where it started so we can set
    /// `Ins.start`.
    pub where_paste_started: PosT,

    // This flag is set after a ":syncbind" to let the check_scrollbind()
    // function know that it should not attempt to perform scrollbinding
    // due to the scroll that was a result of the ":syncbind." (Otherwise,
    // check_scrollbind() will undo some of the work done by
    // ":syncbind.")  -ralston
    pub did_syncbind: bool,

    /// for ^^D command in insert mode
    pub old_indent: i32,

    /// `w_cursor` before formatting text.
    pub saved_cursor: PosT,

    // Stuff for insert mode: the current insert session.
    pub Ins: InsState,

    // Stuff for MODE_VREPLACE state.
    /// Line count when "gR" started
    pub orig_line_count: LinenrT,
    /// #Lines changed by "gR" so far
    pub vr_lines_changed: i32,

    /// increase around internal delete/replace
    pub inhibit_delete_count: i32,

    /// Encoding used when `'fencs'` is set to "default"
    pub fenc_default: Option<Vec<u8>>,

    // "State" is the main state of Vim. There are other variables that
    // modify the state:
    //    Visual_mode:    When State is MODE_NORMAL or MODE_INSERT.
    //    finish_op  :    When State is MODE_NORMAL, after typing the
    //                    operator and before typing the motion command.
    //    motion_force:   Last motion_force from do_pending_operator()
    //    debug_mode:     Debug mode
    pub State: i32,

    pub debug_mode: bool,
    /// true while an operator is pending
    pub finish_op: bool,
    /// count for pending operator
    pub opcount: i32,
    /// motion force for pending operator
    pub motion_force: i32,

    // Ex Mode (Q) state.
    /// true if Ex mode is active
    pub exmode_active: bool,

    /// Flag set when `normal_check()` should return 0 when entering Ex
    /// mode.
    pub pending_exmode_active: bool,

    /// No need to print after z or p.
    pub ex_no_reprint: bool,

    /// `'inccommand'` command preview state
    pub cmdpreview: bool,

    /// register for recording or zero
    pub reg_recording: i32,
    /// register being executed or zero
    pub reg_executing: i32,
    /// Flag set when peeking a character and found the end of executed
    /// register
    pub pending_end_reg_executing: bool,
    /// last recorded register or zero
    pub reg_recorded: i32,

    /// currently no mapping allowed
    pub no_mapping: i32,
    /// mapping zero not allowed
    pub no_zero_mapping: i32,
    /// allow key codes when `no_mapping` is set
    pub allow_keys: i32,
    /// Don't call `u_sync()`
    pub no_u_sync: i32,
    /// Call `u_sync()` once when evaluating an expression.
    pub u_sync_once: i32,

    /// force restart_edit after `ex_normal` returns
    pub force_restart_edit: bool,
    /// call edit when next cmd finished
    pub restart_edit: i32,
    /// put cursor after eol when restarting edit after CTRL-O
    pub ins_at_eol: bool,

    /// true when no abbreviations loaded
    pub no_abbr: bool,

    /// Modes where CTRL-C is mapped.
    pub mapped_ctrl_c: i32,
    /// CTRL-C sets `got_int`
    pub ctrl_c_interrupts: bool,

    /// Ex command modifiers
    pub cmdmod: CmdmodT,

    /// don't print messages
    pub msg_silent: i32,
    /// don't print error messages
    pub emsg_silent: i32,
    /// don't redirect error messages
    pub emsg_noredir: bool,
    /// don't echo the command line
    pub cmd_silent: bool,

    /// `assert_fails()` active
    pub in_assert_fails: bool,

    /// For dialog when swap file already exists.
    pub swap_exists_action: i32,
    /// Selected "quit" at the dialog.
    pub swap_exists_did_quit: bool,

    /// Buffer for sprintf, I/O, etc.
    pub IObuff: [u8; IOSIZE],
    /// Buffer for expanding file names
    pub NameBuff: [u8; MAXPATHL as usize],
    /// Small buffer for messages
    pub msg_buf: [u8; MSG_BUF_LEN],
    /// Buffer for the os/ layer
    pub os_buf: [u8; OS_BUF_SIZE],

    /// When non-zero, postpone redrawing.
    pub RedrawingDisabled: i32,

    /// Set to true for "view"
    pub readonlymode: bool,
    /// Set to true for "-r" option
    pub recoverymode: bool,

    /// typeahead buffer
    pub typebuf: TypebufT,

    /// Flag used to indicate that `vgetorpeek()` returned a char like
    /// Esc when the `:normal` argument was exhausted.
    pub typebuf_was_empty: bool,

    /// recursiveness of `ex_normal()`
    pub ex_normal_busy: i32,
    /// running expr mapping, prevent use of `ex_normal()` and text
    /// changes
    pub expr_map_lock: i32,
    /// ignore script input
    pub ignore_script: bool,
    /// true if user typed current char
    pub KeyTyped: bool,
    /// true if current char from stuffbuf
    pub KeyStuffed: i32,
    /// tick for each non-mapped char
    pub maptick: i32,

    /// type of redraw necessary
    pub must_redraw: i32,
    /// skip redraw once
    pub skip_redraw: bool,
    /// extra redraw once
    pub do_redraw: bool,
    /// redraw pum. NB: `must_redraw` should also be set.
    pub must_redraw_pum: bool,

    pub need_highlight_changed: bool,

    /// Write input to this file (`"nvim -w"`).
    pub scriptout: *mut libc::FILE,

    // Note that even when handling SIGINT, volatile is not necessary
    // because the callback is not called directly from the signal
    // handlers.
    /// set to true when interrupt signal occurred
    pub got_int: bool,
    /// set to true with ! command
    pub bangredo: bool,
    /// Used when compiling regexp: `REX_SET` to allow `\z\(...\)`,
    /// `REX_USE` to allow `\z\1` et al.
    pub reg_do_extmatch: i32,
    /// Used by `vim_regexec()`: strings for `\z\1`...`\z\9`
    pub re_extmatch_in: *mut RegExtmatchT,
    /// Set by `vim_regexec()` to store `\z\(...\)` matches
    pub re_extmatch_out: *mut RegExtmatchT,

    /// set after out of memory msg
    pub did_outofmem_msg: bool,
    /// set after swap write error msg
    pub did_swapwrite_msg: bool,
    /// set when `:global` is executing
    pub global_busy: i32,
    /// set when `:argdo`, `:windo` or `:bufdo` is executing
    pub listcmd_busy: bool,
    /// start insert mode soon
    pub need_start_insertmode: bool,

    pub last_mode: [u8; MODE_MAX_LENGTH],
    /// last command line (for ":)
    pub last_cmdline: Option<Vec<u8>>,
    /// command line for "."
    pub repeat_cmdline: Option<Vec<u8>>,
    /// new value for `last_cmdline`
    pub new_last_cmdline: Option<Vec<u8>>,

    /// for CTRL-W CTRL-] command
    pub postponed_split: i32,
    /// args for `win_split()`
    pub postponed_split_flags: i32,
    /// `cmdmod.cmod_tab`
    pub postponed_split_tab: i32,
    /// for tag preview commands: height of preview window
    pub g_do_tagpreview: i32,
    /// whether the tag command comes from the command line (0) or was
    /// invoked as a normal command (1)
    pub g_tag_at_cursor: bool,

    /// offset for `replace_push()`
    pub replace_offset: i32,

    /// need backslash in cmd line
    pub escape_chars: Vec<u8>,

    /// doing `:ta` from help file
    pub keep_help_flag: bool,

    /// no redirection for a moment
    pub redir_off: bool,
    /// message redirection file
    pub redir_fd: *mut libc::FILE,
    /// message redirection register
    pub redir_reg: i32,
    /// message redirection variable
    pub redir_vname: bool,
    /// captured output for `execute()`
    pub capture_ga: *mut GarrayT,

    /// mapping for language keys
    pub langmap_mapchar: [u8; 256],

    /// Save `'laststatus'` setting
    pub save_p_ls: i32,
    /// Save `'winminheight'` setting
    pub save_p_wmh: i32,
    pub wild_menu_showing: i32,

    // When a window has a local directory, the absolute path of the
    // global current directory is stored here (in allocated memory). If
    // the current directory is not a local directory, globaldir is
    // NULL.
    pub globaldir: Option<Vec<u8>>,

    pub last_chdir_reason: Option<Vec<u8>>,

    // Whether 'keymodel' contains "stopsel" and "startsel".
    pub km_stopsel: bool,
    pub km_startsel: bool,

    /// `|cmdwin|` type (':', '/', '?') or 0.
    pub cmdwin_type: i32,
    /// `|cmdwin|` scratch buffer, or `NULL`.
    pub cmdwin_buf: *mut BufT,
    /// window in use by `ext_cmdline`
    pub cmdline_win: *mut WinT,

    pub no_lines_msg: Vec<u8>,

    // When ":global" is used to number of substitutions and changed
    // lines is accumulated until it's finished. Also used for
    // ":spellrepall".
    /// total number of substitutions
    pub sub_nsubs: i32,
    /// total number of lines changed
    pub sub_nlines: LinenrT,

    /// table to store parsed `'wildmode'`
    pub wim_flags: [u8; 4],

    pub stl_syntax: i32,

    /// received text from client or from `feedkeys()`
    pub typebuf_was_filled: bool,

    // Set to kTrue when an operator is being executed with virtual
    // editing kNone when no operator is being executed, kFalse
    // otherwise.
    pub virtual_op: TriState,

    /// Display tick, incremented for each call to `update_screen()`
    pub display_tick: DisptickT,

    /// Line in which spell checking wasn't highlighted because it
    /// touched the cursor position in Insert mode.
    pub spell_redraw_lnum: LinenrT,

    /// Where to write `--startuptime` report.
    pub time_fd: *mut libc::FILE,

    // Some compilers warn for not using a return value, but in some
    // situations we can't do anything useful with the value. Assign to
    // this variable to avoid the warning.
    pub vim_ignored: i32,

    /// stdio is an RPC channel (--embed).
    pub embedded_mode: bool,
    /// Do not start UI (--headless, -l) nor read/write to stdio (unless
    /// embedding).
    pub headless_mode: bool,

    /// Only filled for Win32.
    pub windowsVersion: [u8; 20],

    /// While executing a regexp and set to `MagicOn`/`MagicOff` this
    /// overrules `p_magic`. Otherwise set to `NotSet`.
    pub magic_overruled: OptmagicT,

    /// Skip `win_fix_scroll()` call for `'splitkeep'` when closing tab
    /// page.
    pub skip_win_fix_scroll: bool,
    /// Skip `update_topline()` call while executing
    /// `win_fix_scroll()`.
    pub skip_update_topline: bool,
}

impl Default for Globals {
    fn default() -> Self {
        Globals {
            g_stats: NvimStatsS::default(),
            Rows: DFLT_ROWS,
            Columns: DFLT_COLS,
            mod_mask: 0,
            vgetc_mod_mask: 0,
            vgetc_char: 0,
            cmdline_row: 0,
            redraw_cmdline: false,
            redraw_mode: false,
            clear_cmdline: false,
            mode_displayed: false,
            cmdline_star: 0,
            redrawing_cmdline: false,
            cmdline_was_last_drawn: false,
            exec_from_reg: false,
            dollar_vcol: -1,
            edit_submode: None,
            edit_submode_pre: None,
            edit_submode_extra: None,
            edit_submode_highl: HlfT::default(),
            cmdmsg_rl: false,
            msg_col: 0,
            msg_row: 0,
            msg_scrolled: 0,
            msg_scrolled_ign: false,
            msg_did_scroll: false,
            keep_msg: None,
            keep_msg_hl_id: 0,
            need_fileinfo: false,
            msg_scroll: 0,
            msg_didout: false,
            msg_didany: false,
            msg_nowait: false,
            emsg_off: 0,
            info_message: false,
            msg_hist_off: false,
            need_clr_eos: false,
            emsg_skip: 0,
            emsg_severe: false,
            emsg_assert_fails_msg: None,
            emsg_assert_fails_lnum: 0,
            emsg_assert_fails_context: None,
            did_endif: false,
            did_emsg: 0,
            called_vim_beep: false,
            did_emsg_syntax: false,
            called_emsg: 0,
            ex_exitval: 0,
            emsg_on_display: false,
            rc_did_emsg: false,
            no_wait_return: 0,
            need_wait_return: false,
            did_wait_return: false,
            need_maketitle: true,
            quit_more: false,
            vgetc_busy: 0,
            didset_vim: false,
            didset_vimruntime: false,
            lines_left: -1,
            msg_no_more: false,
            ex_nesting_level: 0,
            debug_break_level: -1,
            debug_did_msg: false,
            debug_tick: 0,
            debug_backtrace_level: 0,
            do_profiling: 0, // PROF_NONE
            current_exception: std::ptr::null_mut(),
            did_throw: false,
            need_rethrow: false,
            check_cstack: false,
            trylevel: 0,
            force_abort: false,
            msg_list: std::ptr::null_mut(),
            suppress_errthrow: false,
            caught_stack: std::ptr::null_mut(),
            may_garbage_collect: false,
            want_garbage_collect: false,
            garbage_collect_at_exit: false,
            current_sctx: SctxT::default(),
            current_ui: 0,
            did_source_packages: false,
            provider_caller_scope: CallerScope::default(),
            provider_call_nesting: 0,
            t_colors: 256,
            include_none: 0,
            include_default: 0,
            include_link: 0,
            Search: SearchState {
                last_line: MAXLNUM,
                ..SearchState::default()
            },
            need_check_timestamps: false,
            did_check_timestamps: false,
            no_check_timestamps: 0,
            mouse_grid: 0,
            mouse_row: 0,
            mouse_col: 0,
            mouse_past_bottom: false,
            mouse_past_eol: false,
            mouse_dragging: 0,
            root_menu: std::ptr::null_mut(),
            sys_menu: false,
            firstwin: std::ptr::null_mut(),
            lastwin: std::ptr::null_mut(),
            prevwin: std::ptr::null_mut(),
            curwin: std::ptr::null_mut(),
            topframe: std::ptr::null_mut(),
            first_tabpage: std::ptr::null_mut(),
            curtab: std::ptr::null_mut(),
            lastused_tabpage: std::ptr::null_mut(),
            redraw_tabline: false,
            firstbuf: std::ptr::null_mut(),
            lastbuf: std::ptr::null_mut(),
            curbuf: std::ptr::null_mut(),
            global_alist: AlistT::default(),
            max_alist_id: 0,
            arg_had_last: false,
            ru_col: 0,
            ru_wid: 0,
            sc_col: 0,
            starting: NO_SCREEN,
            exiting: false,
            v_dying: 0,
            stdin_isatty: true,
            stdout_isatty: true,
            stderr_isatty: true,
            stdin_fd: -1,
            full_screen: false,
            secure: 0,
            textlock: 0,
            allbuf_lock: 0,
            sandbox: 0,
            silent_mode: false,
            Visual: VisualState {
                mode: b'v' as i32,
                ..VisualState::default()
            },
            where_paste_started: PosT::default(),
            did_syncbind: false,
            old_indent: 0,
            saved_cursor: PosT::default(),
            Ins: InsState::default(),
            orig_line_count: 0,
            vr_lines_changed: 0,
            inhibit_delete_count: 0,
            fenc_default: None,
            State: mode::NORMAL as i32,
            debug_mode: false,
            finish_op: false,
            opcount: 0,
            motion_force: 0,
            exmode_active: false,
            pending_exmode_active: false,
            ex_no_reprint: false,
            cmdpreview: false,
            reg_recording: 0,
            reg_executing: 0,
            pending_end_reg_executing: false,
            reg_recorded: 0,
            no_mapping: 0,
            no_zero_mapping: 0,
            allow_keys: 0,
            no_u_sync: 0,
            u_sync_once: 0,
            force_restart_edit: false,
            restart_edit: 0,
            ins_at_eol: false,
            no_abbr: true,
            mapped_ctrl_c: 0,
            ctrl_c_interrupts: true,
            cmdmod: CmdmodT::default(),
            msg_silent: 0,
            emsg_silent: 0,
            emsg_noredir: false,
            cmd_silent: false,
            in_assert_fails: false,
            swap_exists_action: SEA_NONE,
            swap_exists_did_quit: false,
            IObuff: [0; IOSIZE],
            NameBuff: [0; MAXPATHL as usize],
            msg_buf: [0; MSG_BUF_LEN],
            os_buf: [0; OS_BUF_SIZE],
            RedrawingDisabled: 0,
            readonlymode: false,
            recoverymode: false,
            typebuf: TypebufT::default(),
            typebuf_was_empty: false,
            ex_normal_busy: 0,
            expr_map_lock: 0,
            ignore_script: false,
            KeyTyped: false,
            KeyStuffed: 0,
            maptick: 0,
            must_redraw: 0,
            skip_redraw: false,
            do_redraw: false,
            must_redraw_pum: false,
            need_highlight_changed: true,
            scriptout: std::ptr::null_mut(),
            got_int: false,
            bangredo: false,
            reg_do_extmatch: 0,
            re_extmatch_in: std::ptr::null_mut(),
            re_extmatch_out: std::ptr::null_mut(),
            did_outofmem_msg: false,
            did_swapwrite_msg: false,
            global_busy: 0,
            listcmd_busy: false,
            need_start_insertmode: false,
            last_mode: [b'n', 0, 0, 0],
            last_cmdline: None,
            repeat_cmdline: None,
            new_last_cmdline: None,
            postponed_split: 0,
            postponed_split_flags: 0,
            postponed_split_tab: 0,
            g_do_tagpreview: 0,
            g_tag_at_cursor: false,
            replace_offset: 0,
            escape_chars: b" \t\\\"|".to_vec(),
            keep_help_flag: false,
            redir_off: false,
            redir_fd: std::ptr::null_mut(),
            redir_reg: 0,
            redir_vname: false,
            capture_ga: std::ptr::null_mut(),
            langmap_mapchar: [0; 256],
            save_p_ls: -1,
            save_p_wmh: -1,
            wild_menu_showing: 0,
            globaldir: None,
            last_chdir_reason: None,
            km_stopsel: false,
            km_startsel: false,
            cmdwin_type: 0,
            cmdwin_buf: std::ptr::null_mut(),
            cmdline_win: std::ptr::null_mut(),
            no_lines_msg: gettext_noop("--No lines in buffer--").as_bytes().to_vec(),
            sub_nsubs: 0,
            sub_nlines: 0,
            wim_flags: [0; 4],
            stl_syntax: 0,
            typebuf_was_filled: false,
            virtual_op: TriState::None,
            display_tick: 0,
            spell_redraw_lnum: 0,
            time_fd: std::ptr::null_mut(),
            vim_ignored: 0,
            embedded_mode: false,
            headless_mode: false,
            windowsVersion: [0; 20],
            magic_overruled: OptmagicT::default(),
            skip_win_fix_scroll: false,
            skip_update_topline: false,
        }
    }
}

/// Wrapper providing the original's "one process-wide instance,
/// reachable from any function with no explicit parameter" access
/// pattern for [`Globals`] (see the module-level doc comment for why
/// this crate keeps that architecture rather than threading an explicit
/// parameter through every future translated function).
///
/// Neovim's C code assumes a single-threaded main loop touches this
/// state (the same assumption every `EXTERN` global in the original
/// relies on); this wrapper provides that exact same "trust the caller"
/// contract via [`UnsafeCell`] rather than a bare `static mut` reference
/// (increasingly restricted/lint-denied in modern Rust editions) or a
/// `Mutex`/`RefCell` (which would add a runtime borrow-check/lock that
/// doesn't exist in the original and could newly panic/deadlock on
/// reentrant access patterns the original's C code does have, e.g.
/// autocmd callbacks touching globals while already inside a global-
/// touching function).
///
/// Generic over `T` so it can back not just [`Globals`] (this file's own
/// `EXTERN` variables) but any other single-file `EXTERN` global
/// elsewhere in the codebase - e.g. `mark.h`'s `EXTERN xfmark_T
/// namedfm[NGLOBALMARKS]` (see `crate::mark`) - without duplicating this
/// same small amount of unsafe boilerplate at every call site.
pub struct GlobalCell<T>(UnsafeCell<T>);

// SAFETY: matches the original's own single-threaded-main-loop
// assumption; every `EXTERN` global in the original C is likewise never
// synchronized for multi-threaded access.
unsafe impl<T> Sync for GlobalCell<T> {}
unsafe impl<T> Send for GlobalCell<T> {}

impl<T> GlobalCell<T> {
    /// Wraps `value` for use in a `static` initializer (typically inside
    /// a [`std::sync::LazyLock::new`] closure, since most `Globals`-like
    /// values aren't `const`-constructible).
    pub const fn new(value: T) -> Self {
        GlobalCell(UnsafeCell::new(value))
    }

    /// Returns a mutable reference to the single global value.
    ///
    /// # Safety
    /// Caller must not create two overlapping live references from this
    /// method (e.g. holding one across a call that itself calls this
    /// again) - the same non-reentrant-aliasing invariant the original's
    /// single-threaded design already required of every function that
    /// touches the corresponding `EXTERN` global.
    #[allow(clippy::mut_from_ref)]
    pub unsafe fn get_mut(&self) -> &mut T {
        unsafe { &mut *self.0.get() }
    }
}

/// [`GlobalCell`] specialized for [`Globals`] itself.
pub type GlobalsCell = GlobalCell<Globals>;

/// The single, global, mutable editor state instance - the Rust
/// equivalent of `globals.h`'s `EXTERN` mechanism (every `EXTERN`
/// variable is declared in the header and defined exactly once, in
/// `globals.h`'s own translation unit via `#define EXTERN` tricks in
/// `main.c`; here it's simply one `static`).
///
/// Lazily initialized (via [`std::sync::LazyLock`]) rather than a
/// direct `const` initializer, since `Globals::default()` (and its
/// field types' own `Default` impls) aren't `const fn` - making all of
/// them `const fn` purely to allow a non-lazy `static` would be a much
/// larger, riskier cross-cutting change for no behavioral benefit here.
pub static GLOBALS: std::sync::LazyLock<GlobalsCell> =
    std::sync::LazyLock::new(|| GlobalCell::new(Globals::default()));

/// Serializes tests, in *any* file in this crate, that read or write
/// [`GLOBALS`]/[`crate::option_vars::OPTION_VARS`] (or any other
/// [`GlobalCell`]) via `.get_mut()`. `GlobalCell` itself provides zero
/// synchronization (`unsafe impl Sync` - a deliberate, faithful match
/// for the original's own single-threaded, unsynchronized `EXTERN`
/// globals), so two tests running concurrently on different threads
/// (Rust's default test runner) and both calling `.get_mut()` create
/// overlapping `&mut` references from different threads - undefined
/// behavior, not just "a wrong assertion", regardless of whether the
/// two tests happen to touch different fields of the same struct.
///
/// This is not a hypothetical: this exact class of bug was caught for
/// real by running this crate's test suite natively on Linux for the
/// first time (via WSL) - `mark.rs`'s `MarkTestGuard`/`CurbufGuard`
/// mutated `GLOBALS.curwin`/`curbuf` with no synchronization at all,
/// and Linux's thread scheduling surfaced the resulting race
/// reliably (never observed across dozens of Windows-only runs).
/// Auditing the rest of the crate afterwards found the same
/// unprotected pattern in `memfile.rs`'s `did_swapwrite_msg`/`got_int`
/// tests too - fixed alongside this lock's introduction.
///
/// Every test anywhere in the crate that calls `GLOBALS.get_mut()` or
/// `crate::option_vars::OPTION_VARS.get_mut()` should acquire this for
/// its entire body (held via the returned guard, dropped at the end of
/// the test function). Uses `PoisonError::into_inner` so one
/// panicking test under the lock can't permanently poison it for every
/// later test - same pattern as `crate::os::fs::cwd_test_lock`/
/// `os::env`'s `homedir_test_lock`.
#[cfg(test)]
pub(crate) fn global_state_test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globals_default_matches_c_init_values() {
        let g = Globals::default();
        assert_eq!(g.Rows, 24);
        assert_eq!(g.Columns, 80);
        assert_eq!(g.dollar_vcol, -1);
        assert!(g.need_maketitle);
        assert_eq!(g.lines_left, -1);
        assert_eq!(g.debug_break_level, -1);
        assert_eq!(g.t_colors, 256);
        assert_eq!(g.Search.last_line, MAXLNUM);
        assert_eq!(g.starting, NO_SCREEN);
        assert!(g.stdin_isatty);
        assert_eq!(g.stdin_fd, -1);
        assert_eq!(g.Visual.mode, b'v' as i32);
        assert_eq!(g.State, mode::NORMAL as i32);
        assert!(g.no_abbr);
        assert!(g.ctrl_c_interrupts);
        assert_eq!(g.swap_exists_action, SEA_NONE);
    }

    #[test]
    fn globals_default_has_null_current_state_pointers() {
        let g = Globals::default();
        assert!(g.curwin.is_null());
        assert!(g.curbuf.is_null());
        assert!(g.curtab.is_null());
        assert!(g.topframe.is_null());
        assert!(g.current_exception.is_null());
        assert!(g.caught_stack.is_null());
        assert!(g.msg_list.is_null());
        assert!(g.root_menu.is_null());
    }

    #[test]
    fn buffer_sizes_match_c_macros() {
        assert_eq!(IOSIZE, 1025);
        assert_eq!(MSG_BUF_LEN, 480);
        assert_eq!(MSG_BUF_CLEN, 80);
        assert!(OS_BUF_SIZE == MAXPATHL as usize || OS_BUF_SIZE == IOSIZE);
    }

    #[test]
    fn sid_constants_are_all_distinct_negatives() {
        let all = [
            SID_MODELINE,
            SID_CMDARG,
            SID_CARG,
            SID_ENV,
            SID_ERROR,
            SID_NONE,
            SID_WINLAYOUT,
            SID_LUA,
            SID_API_CLIENT,
            SID_STR,
        ];
        for &v in &all {
            assert!(v < 0);
        }
        for i in 0..all.len() {
            for j in (i + 1)..all.len() {
                assert_ne!(all[i], all[j]);
            }
        }
    }

    #[test]
    fn caller_scope_default_has_null_funccalp() {
        let cs = CallerScope::default();
        assert!(cs.funccalp.is_null());
        assert!(cs.autocmd_fname.is_none());
    }

    #[test]
    fn globals_singleton_is_readable_and_writable() {
        // Uses global_state_test_lock() for its entire body: GLOBALS is
        // a genuine process-wide singleton (matching the original's own
        // architecture - see the GlobalsCell doc comment) with zero
        // built-in synchronization, so without this lock, a concurrently
        // running test that also touches GLOBALS could create
        // overlapping &mut references from a different thread -
        // undefined behavior, not just a wrong assertion (see
        // global_state_test_lock's own doc comment for a real instance
        // of this class of bug, caught via native Linux testing).
        let _lock = global_state_test_lock();
        let original_rows = unsafe { GLOBALS.get_mut() }.Rows;
        unsafe { GLOBALS.get_mut() }.Rows = 50;
        assert_eq!(unsafe { GLOBALS.get_mut() }.Rows, 50);
        unsafe { GLOBALS.get_mut() }.Rows = original_rows;
    }
}
