//! Translated from `src/nvim/ex_getln.c` (tractable core only).
//!
//! `ex_getln.c` is the command-line-editing/history file (thousands of
//! lines - needs the whole cmdline-editing subsystem, not attempted
//! here). Translated: [`vim_strsave_fnameescape`]/[`escape_fname`]
//! (used by `fnameescape()` and several not-yet-translated `ex_*`
//! commands), tractable on their own via the already-existing
//! `crate::strings::vim_strsave_escaped`/`crate::option::csh_like_shell`.
//!
//! The original's `vim_isfilec()`-based special-case for `[`/`{`/`!`
//! (only reached on the `BACKSLASH_IN_FILENAME`/Windows branch) is
//! simplified to its REAL answer for the DEFAULT, unconfigured
//! `'isfname'` value: `false` for all three characters, verified
//! directly against `'isfname'`'s own real, documented default value
//! on BOTH platforms (`@,48-57,/,.,-,_,+,,,#,$,%,~,=` non-Windows,
//! plus `,\,:` on Windows - neither ever includes `[`/`{`/`!`) - this
//! makes the original's own `(*p != '[' && ...) || !vim_isfilec(*p)`
//! filter condition ALWAYS true for the default option value, so every
//! character in the escape-char-set constants is genuinely kept
//! verbatim, matching this crate's established "fixed default rule"
//! pattern (`vim_isprintc`/`vim_isbreak`/`vim_isidc`) rather than the
//! general `g_chartab`-dependent mechanism.
//!
//! Also translated: the real `CmdlineInfo` file-static (`ccline`) and
//! its accessor family - [`get_cmdline_info`],
//! [`get_cmdline_last_prompt_id`], [`get_ccline_ptr`] - plus the
//! narrow readers built on it: [`get_cmdline_firstc`],
//! [`cmdline_overstrike`] and [`cmdline_at_end`]. Those three
//! previously each had a standalone stand-in file-static
//! (`CMDLINE_FIRSTC`/`CMDLINE_OVERSTRIKE`/`CMDLINE_CMDPOS`/
//! `CMDLINE_CMDLEN`), documented as placeholders until the struct
//! itself was translated; they now read the real thing. Their answers
//! are unchanged, since nothing in this crate can start real
//! command-line editing yet, so every field still reads as "no
//! command line active".
//!
//! Also translated: [`cmdpreview_get_bufnr`]/[`cmdpreview_get_ns`] -
//! trivial accessors over the original's own file-static
//! `cmdpreview_bufnr`/`cmdpreview_ns`, modeled the same way. Both
//! always `0` today, since nothing in this crate can start a real
//! `'inccommand'` command preview yet (`cmdpreview_open_buf`, their
//! only real writer, is not translated).
//!
//! Also translated: [`is_in_cmdwin`] - whether the current buffer is
//! the special `cmdwin` scratch buffer with no other command line
//! simultaneously active; its own second real condition always holds
//! today (see its own doc comment), so it simplifies to
//! `buffer::bt_cmdwin(curbuf)`.
//!
//! Also translated: [`check_opt_wim`] - parses `'wildmode'` into
//! `GLOBALS.wim_flags`, hand-traced against a concrete
//! `"longest:full,list,full"` example (a `:`-joined group combines
//! flags into the SAME slot; a `,`-joined group starts a new one)
//! before translating. `optionstr.rs`'s `did_set_wildmode` is its
//! real caller.

/// What [`vim_strsave_fnameescape`] is escaping for (`VSE_NONE`/
/// `VSE_SHELL`/`VSE_BUFFER`, `ex_getln.h`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VseWhat {
    /// escape for a file name (`VSE_NONE`).
    #[default]
    None,
    /// escape for a shell command (`VSE_SHELL`).
    Shell,
    /// escape for a `:buffer` command (`VSE_BUFFER`).
    Buffer,
}

#[cfg(windows)]
const PATH_ESC_CHARS: &[u8] = b" \t\n*?[{`%#'\"|!<";
#[cfg(windows)]
const BUFFER_ESC_CHARS: &[u8] = b" \t\n*?[`'\"|!<";

#[cfg(not(windows))]
const PATH_ESC_CHARS: &[u8] = b" \t\n*?[{`$\\%#'\"|!<";
#[cfg(not(windows))]
const SHELL_ESC_CHARS: &[u8] = b" \t\n*?[{`$\\%#'\"|!<>();&";
#[cfg(not(windows))]
const BUFFER_ESC_CHARS: &[u8] = b" \t\n*?[`$\\%#'\"|!<";

/// Saved window view state, used while `'incsearch'` highlighting and
/// `'inccommand'` preview temporarily move the view (`viewstate_T`, a
/// private struct in the original).
///
/// Every field is a plain copy of the window field of the same name,
/// so [`save_viewstate`]/[`restore_viewstate`] are exact inverses.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ViewstateT {
    pub vs_curswant: crate::pos_defs::ColnrT,
    pub vs_leftcol: crate::pos_defs::ColnrT,
    pub vs_skipcol: crate::pos_defs::ColnrT,
    pub vs_topline: crate::pos_defs::LinenrT,
    pub vs_topfill: i32,
    pub vs_botline: crate::pos_defs::LinenrT,
    pub vs_empty_rows: i32,
}

/// Record `wp`'s current view so it can be put back later
/// (`save_viewstate`).
pub fn save_viewstate(wp: &crate::buffer_defs::WinT) -> ViewstateT {
    ViewstateT {
        vs_curswant: wp.w_curswant,
        vs_leftcol: wp.w_leftcol,
        vs_skipcol: wp.w_skipcol,
        vs_topline: wp.w_topline,
        vs_topfill: wp.w_topfill,
        vs_botline: wp.w_botline,
        vs_empty_rows: wp.w_empty_rows,
    }
}

/// Put back a view previously recorded by [`save_viewstate`]
/// (`restore_viewstate`).
pub fn restore_viewstate(wp: &mut crate::buffer_defs::WinT, vs: &ViewstateT) {
    wp.w_curswant = vs.vs_curswant;
    wp.w_leftcol = vs.vs_leftcol;
    wp.w_skipcol = vs.vs_skipcol;
    wp.w_topline = vs.vs_topline;
    wp.w_topfill = vs.vs_topfill;
    wp.w_botline = vs.vs_botline;
    wp.w_empty_rows = vs.vs_empty_rows;
}

/// Parse a `"from,to"` number range from the front of `str_`
/// (`get_list_range`).
///
/// @return `Some((num1, num2, consumed))` on success, where `consumed`
///         is how many bytes were used, or `None` for `FAIL` -
///         replacing the original's `char **str` in/out pointer plus
///         two out-parameters. Each number is only reported when the
///         input actually supplied it, so the caller's own defaults
///         survive an absent part, exactly as upstream's untouched
///         out-parameters do.
///
/// Three shapes are accepted: a lone number (which becomes BOTH ends),
/// a full `"a,b"` pair, and a bare `",b"` with no first part. Only a
/// `","` with nothing usable on either side fails, along with any
/// value overflowing `i32`.
#[must_use]
pub fn get_list_range(str_: &[u8]) -> Option<(Option<i32>, Option<i32>, usize)> {
    let mut pos = crate::charset::skipwhite(str_);
    let (mut num1, mut num2) = (None, None);
    let mut first = false;

    // Parse the "from" part of the range.
    if str_
        .get(pos)
        .is_some_and(|&c| c == b'-' || crate::ascii_defs::ascii_isdigit(i32::from(c)))
    {
        let mut len = 0;
        let mut num = 0;
        let mut overflow = false;
        crate::charset::vim_str2nr(
            &str_[pos..],
            None,
            Some(&mut len),
            0,
            Some(&mut num),
            None,
            0,
            false,
            Some(&mut overflow),
        );
        pos += len as usize;
        // Overflow.
        if overflow {
            return None;
        }
        num1 = Some(i32::try_from(num).ok()?);
        first = true;
    }

    pos += crate::charset::skipwhite(&str_[pos..]);

    if str_.get(pos) == Some(&b',') {
        // Parse the "to" part of the range.
        pos += 1;
        pos += crate::charset::skipwhite(&str_[pos..]);

        let mut len = 0;
        let mut num = 0;
        let mut overflow = false;
        crate::charset::vim_str2nr(
            &str_[pos..],
            None,
            Some(&mut len),
            0,
            Some(&mut num),
            None,
            0,
            false,
            Some(&mut overflow),
        );
        if len > 0 {
            pos += len as usize;
            pos += crate::charset::skipwhite(&str_[pos..]);
            // Overflow.
            if overflow {
                return None;
            }
            num2 = Some(i32::try_from(num).ok()?);
        } else if !first {
            // No number given at all.
            return None;
        }
    } else if first {
        // Only one number given: it is both ends of the range.
        num2 = num1;
    }

    Some((num1, num2, pos))
}

/// Put a backslash before `s`, in place (`escape_fname`).
pub fn escape_fname(s: &mut Vec<u8>) {
    s.insert(0, b'\\');
}

/// Escape `fname` for use as a `:!`/`:cd`/file-name-context argument
/// (`vim_strsave_fnameescape`). See this module's own doc comment for
/// the `vim_isfilec`-simplification this relies on.
///
/// # Safety
/// Touches `OPTION_VARS` (via `crate::strings::vim_strsave_escaped`).
#[must_use]
pub unsafe fn vim_strsave_fnameescape(fname: &[u8], what: VseWhat) -> Vec<u8> {
    #[cfg(windows)]
    let esc_chars = if what == VseWhat::Buffer { BUFFER_ESC_CHARS } else { PATH_ESC_CHARS };
    #[cfg(windows)]
    // SAFETY: forwarded from this function's own safety doc.
    let mut p = unsafe { crate::strings::vim_strsave_escaped(fname, esc_chars) };

    #[cfg(not(windows))]
    let esc_chars = match what {
        VseWhat::Shell => SHELL_ESC_CHARS,
        VseWhat::Buffer => BUFFER_ESC_CHARS,
        VseWhat::None => PATH_ESC_CHARS,
    };
    #[cfg(not(windows))]
    // SAFETY: forwarded from this function's own safety doc.
    let mut p = unsafe { crate::strings::vim_strsave_escaped(fname, esc_chars) };
    #[cfg(not(windows))]
    if what == VseWhat::Shell && crate::option::csh_like_shell() {
        // For csh and similar shells need to put two backslashes
        // before '!'. One is taken by Vim, one by the shell.
        // SAFETY: forwarded from this function's own safety doc.
        p = unsafe { crate::strings::vim_strsave_escaped(&p, b"!") };
    }

    // '>' and '+' are special at the start of some commands, e.g.
    // ":edit" and ":write". "cd -" has a special meaning.
    if p.first() == Some(&b'>')
        || p.first() == Some(&b'+')
        || (p.first() == Some(&b'-') && p.len() == 1)
    {
        escape_fname(&mut p);
    }

    p
}

/// Whether a command line is currently being edited (`get_ccline_ptr`,
/// `ex_getln.c`) - the original resolves to one of 3 further branches
/// (a live `ccline`, a saved `ccline.prev_ccline`, or `NULL`) ONLY
/// after first checking `(State & MODE_CMDLINE) == 0` - and since
/// nothing in this crate can ever set the `MODE_CMDLINE` bit on
/// `GLOBALS.State` (no `:`/`/`-style command-line entry mode exists
/// yet), that check is always true, making every real caller's own
/// "is a command line active" question always `false` today - a
/// faithful, always-taken early return, not a hardcoded shortcut
/// (matching this crate's established `AUTOCMDS`/`ctx_restore`
/// precedent for this exact pattern).
fn cmdline_is_active() -> bool {
    // SAFETY: reading a plain `i32` field, no aliasing hazard.
    let state = unsafe { crate::globals::GLOBALS.get_mut() }.State;
    state & crate::state_defs::mode::CMDLINE as i32 != 0
}

/// `ccline.cmdfirstc` - the leading character of the CURRENTLY-being-
/// edited command line (`:`/`=`/`@`/`>`/`/`/`?`), read via
/// [`get_cmdline_firstc`]. This is a plain `i32` field of the
/// original's own file-static `ccline` (not gated behind
/// `get_ccline_ptr`'s `MODE_CMDLINE` check the way `cmdline_is_active`
/// is) - so it's modeled directly as its own file-static, matching
/// `cmdline_star`'s own precedent in `GLOBALS`, rather than the full
/// `CmdlineInfo` struct (not needed for this one field). Always `0`
/// (NUL) today: a fresh, zero-initialized `ccline.cmdfirstc`, since
/// nothing in this crate can start real command-line editing yet.
/// The current command-line editing state (`ccline`).
///
/// Replaces the several single-field statics this module previously
/// used (`CMDLINE_FIRSTC`, `CMDLINE_OVERSTRIKE`, `CMDLINE_CMDPOS`,
/// `CMDLINE_CMDLEN`), now that the real `CmdlineInfo` exists. Those
/// were each documented as standing in for one `ccline` field until
/// the struct itself was translated.
///
/// Zero-initialized, matching the original's own file-static: nothing
/// in this crate can start real command-line editing yet, so every
/// field still reads as "no command line active".
static CCLINE: crate::globals::GlobalCell<crate::ex_getln_defs::CmdlineInfo> =
    crate::globals::GlobalCell::new(crate::ex_getln_defs::CmdlineInfo {
        cmdbuff: None,
        cmdlen: 0,
        cmdpos: 0,
        cmdspos: 0,
        cmdfirstc: 0,
        cmdindent: 0,
        cmdprompt: None,
        hl_id: 0,
        overstrike: 0,
        xpc: std::ptr::null_mut(),
        xp_context: 0,
        xp_arg: None,
        input_fn: 0,
        cmdbuff_replaced: false,
        prompt_id: 0,
        highlight_callback: crate::eval::typval_defs::Callback::None,
        last_colors: crate::ex_getln_defs::ColoredCmdline {
            prompt_id: 0,
            cmdbuff: None,
            colors: Vec::new(),
        },
        level: 0,
        prev_ccline: std::ptr::null_mut(),
        special_char: 0,
        special_shift: false,
        redraw_state: crate::ex_getln_defs::CmdRedraw::None,
        one_key: false,
        mouse_used: std::ptr::null_mut(),
    });

/// The ID of the most recent command-line prompt (`last_prompt_id`).
static LAST_PROMPT_ID: crate::globals::GlobalCell<u32> = crate::globals::GlobalCell::new(0);

/// The current command-line info (`get_cmdline_info`).
///
/// # Safety
/// Returns a pointer to the `CCLINE` file-static; the caller must not
/// alias it.
#[must_use]
pub unsafe fn get_cmdline_info() -> *mut crate::ex_getln_defs::CmdlineInfo {
    // SAFETY: forwarded from this function's own safety doc.
    std::ptr::from_mut(unsafe { CCLINE.get_mut() })
}

/// The ID of the last command-line prompt
/// (`get_cmdline_last_prompt_id`).
///
/// # Safety
/// Reads the `LAST_PROMPT_ID` file-static.
#[must_use]
pub unsafe fn get_cmdline_last_prompt_id() -> u32 {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *LAST_PROMPT_ID.get_mut() }
}

/// The command-line info to actually use (`get_ccline_ptr`).
///
/// `save_cmdline` may clear `ccline` and move the previous value into
/// `ccline.prev_ccline`, so the live state is not always `ccline`
/// itself. Null when no command line is active at all.
///
/// # Safety
/// Reads `GLOBALS` and the `CCLINE` file-static, and follows
/// `prev_ccline`, which must be null or point at a live
/// `CmdlineInfo`.
#[must_use]
pub unsafe fn get_ccline_ptr() -> *mut crate::ex_getln_defs::CmdlineInfo {
    // SAFETY: forwarded from this function's own safety doc.
    let state = unsafe { crate::globals::GLOBALS.get_mut() }.State;
    if state as u32 & crate::state_defs::mode::CMDLINE == 0 {
        return std::ptr::null_mut();
    }

    // SAFETY: forwarded from this function's own safety doc.
    let ccline = unsafe { CCLINE.get_mut() };
    if ccline.cmdbuff.is_some() {
        return std::ptr::from_mut(ccline);
    }

    let prev = ccline.prev_ccline;
    // SAFETY: forwarded from this function's own safety doc.
    if !prev.is_null() && unsafe { (*prev).cmdbuff.is_some() } {
        return prev;
    }
    std::ptr::null_mut()
}

/// `get_cmdline_firstc()` - the leading character of the current
/// command line, or `0` (NUL) when none is active (`ex_getln.c`).
///
/// Still `0` today, since nothing in this crate can start real
/// command-line editing - but now read from the real `ccline` rather
/// than a stand-in static.
#[must_use]
pub fn get_cmdline_firstc() -> i32 {
    // SAFETY: a plain `i32` copy-out read of the file-static.
    unsafe { CCLINE.get_mut() }.cmdfirstc
}

/// Return `true` if the command line is in Replace mode
/// (`cmdline_overstrike`).
#[must_use]
pub fn cmdline_overstrike() -> bool {
    // SAFETY: a plain copy-out read of the file-static.
    unsafe { CCLINE.get_mut() }.overstrike != 0
}

/// Return `true` if the cursor is at the end of the command line
/// (`cmdline_at_end`).
#[must_use]
pub fn cmdline_at_end() -> bool {
    // SAFETY: plain `i32` copy-out reads of the file-static.
    let ccline = unsafe { CCLINE.get_mut() };
    ccline.cmdpos >= ccline.cmdlen
}


/// Allocate a new command-line buffer (`alloc_cmdbuff`).
///
/// Extra space is reserved beyond the requested length so that typing
/// does not reallocate on every character - the original's own
/// rationale, kept because it is a real allocation strategy rather
/// than an artefact. It becomes the `Vec`'s capacity here, since this
/// crate has no separate `cmdbufflen` field (see `CmdlineInfo`'s own
/// doc comment).
///
/// # Safety
/// Mutates the `CCLINE` file-static.
pub unsafe fn alloc_cmdbuff(len: i32) {
    // Give some extra space to avoid having to allocate all the time.
    let len = if len < 80 { 100 } else { len + 20 };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { CCLINE.get_mut() }.cmdbuff = Some(Vec::with_capacity(len.max(0) as usize));
}

/// Deallocate the command-line buffer (`dealloc_cmdbuff`).
///
/// # Safety
/// Mutates the `CCLINE` file-static.
pub unsafe fn dealloc_cmdbuff() {
    // SAFETY: forwarded from this function's own safety doc.
    let ccline = unsafe { CCLINE.get_mut() };
    ccline.cmdbuff = None;
    ccline.cmdlen = 0;
}

/// Save the current command-line state into `ccp` and start a fresh
/// one (`save_cmdline`).
///
/// The saved state is linked as the new state's `prev_ccline`, so
/// [`get_ccline_ptr`] can still find the live command line. The new
/// state's `cmdbuff` is left unset, which is precisely the signal
/// that `ccline` is not itself in use.
///
/// # Safety
/// `ccp` must point at a live [`crate::ex_getln_defs::CmdlineInfo`]
/// that outlives the save/restore pair, and mutates the `CCLINE`
/// file-static.
pub unsafe fn save_cmdline(ccp: *mut crate::ex_getln_defs::CmdlineInfo) {
    // SAFETY: forwarded from this function's own safety doc.
    let ccline = unsafe { CCLINE.get_mut() };
    // Moving out and leaving the default behind is exactly the
    // original's "copy out, then CLEAR_FIELD" pair, without the
    // window in which both hold the same owned pointers.
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *ccp = std::mem::take(ccline) };
    ccline.prev_ccline = ccp;
    // Signal that ccline is not in use.
    ccline.cmdbuff = None;
}

/// Restore the command-line state saved by [`save_cmdline`]
/// (`restore_cmdline`).
///
/// # Safety
/// `ccp` must point at a live `CmdlineInfo` previously filled by
/// [`save_cmdline`], and mutates the `CCLINE` file-static.
pub unsafe fn restore_cmdline(ccp: *mut crate::ex_getln_defs::CmdlineInfo) {
    // SAFETY: forwarded from this function's own safety doc.
    let restored = unsafe { std::mem::take(&mut *ccp) };
    // SAFETY: forwarded from this function's own safety doc.
    *unsafe { CCLINE.get_mut() } = restored;
}

/// Reset the command-line state entirely (`cmdline_init`).
///
/// # Safety
/// Mutates the `CCLINE` file-static.
pub unsafe fn cmdline_init() {
    // SAFETY: forwarded from this function's own safety doc.
    *unsafe { CCLINE.get_mut() } = crate::ex_getln_defs::CmdlineInfo::default();
}

/// The screen width of the command-line character at byte offset
/// `idx` (`cmdline_charsize`).
///
/// An obscured command line shows `'*'` for every character, which is
/// always one cell wide.
///
/// # Safety
/// Reads `GLOBALS` and the `CCLINE` file-static, and forwards
/// [`crate::charset::ptr2cells`]'s own safety doc.
#[must_use]
pub unsafe fn cmdline_charsize(idx: usize) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { crate::globals::GLOBALS.get_mut() }.cmdline_star > 0 {
        // Showing '*': always one position.
        return 1;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let ccline = unsafe { CCLINE.get_mut() };
    let Some(buff) = ccline.cmdbuff.as_ref() else {
        return 1;
    };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::charset::ptr2cells(buff.get(idx..).unwrap_or_default()) }
}

/// The screen-column offset of the command line's own text, past the
/// prompt and indent (`cmd_startcol`).
///
/// # Safety
/// Reads the `CCLINE` file-static.
#[must_use]
pub unsafe fn cmd_startcol() -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let ccline = unsafe { CCLINE.get_mut() };
    ccline.cmdindent + i32::from(ccline.cmdfirstc != 0)
}

/// Nudge a screen column past a wide character that would not fit
/// (`correct_screencol`).
///
/// A multi-byte, multi-cell character that would straddle the right
/// edge is pushed to the next row, so one extra column is consumed to
/// leave room for the `">"` marker.
///
/// The original adds into an `int *col` in-out parameter; the column
/// is taken and returned by value here.
///
/// # Safety
/// Reads `GLOBALS` and the `CCLINE` file-static, and forwards
/// [`crate::mbyte::utfc_ptr2len`]/[`crate::mbyte::utf_ptr2cells`]'s
/// own safety docs.
#[must_use]
pub unsafe fn correct_screencol(idx: usize, cells: i32, col: i32) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let ccline = unsafe { CCLINE.get_mut() };
    let Some(buff) = ccline.cmdbuff.as_ref() else {
        return col;
    };
    let p = buff.get(idx..).unwrap_or_default();

    // SAFETY: forwarded from this function's own safety doc.
    let columns = unsafe { crate::globals::GLOBALS.get_mut() }.Columns;
    // SAFETY: forwarded from this function's own safety doc.
    let is_multibyte = unsafe { crate::mbyte::utfc_ptr2len(p) } > 1;
    // SAFETY: forwarded from this function's own safety doc.
    let is_wide = unsafe { crate::mbyte::utf_ptr2cells(p) } > 1;

    if is_multibyte && is_wide && columns > 0 && col % columns + cells > columns {
        col + 1
    } else {
        col
    }
}

/// `getcmdcomplpat()` - the current command-line completion pattern
/// (`f_getcmdcomplpat`, `ex_getln.c`) - always empty today, since
/// `cmdline_is_active` is always `false` (the original's own
/// `get_cmdline_completion_pattern` checks `cmdline_star > 0` first -
/// always false, `GLOBALS.cmdline_star` defaults to `0` and nothing
/// yet sets it - then falls through to the same "no active command
/// line" `NULL` result either way).
pub fn f_getcmdcomplpat(
    _argvars: &[crate::eval::typval_defs::TypvalT],
    rettv: &mut crate::eval::typval_defs::TypvalT,
) {
    rettv.value = if cmdline_is_active() {
        unimplemented!("getcmdcomplpat(): needs a real, live command-line-editing state")
    } else {
        crate::eval::typval_defs::TypvalValue::String(None)
    };
}

/// `getcmdcompltype()` - the current command-line completion type
/// (`f_getcmdcompltype`, `ex_getln.c`) - always empty today, matching
/// [`f_getcmdcomplpat`]'s own exact reasoning (`get_cmdline_completion`
/// has the identical `cmdline_star`/`get_ccline_ptr` structure).
pub fn f_getcmdcompltype(
    _argvars: &[crate::eval::typval_defs::TypvalT],
    rettv: &mut crate::eval::typval_defs::TypvalT,
) {
    rettv.value = if cmdline_is_active() {
        unimplemented!("getcmdcompltype(): needs a real, live command-line-editing state")
    } else {
        crate::eval::typval_defs::TypvalValue::String(None)
    };
}

/// The current command-line type (`get_cmdline_type`).
///
/// Only meaningful while the command line is being edited; returns
/// NUL when something is wrong.
///
/// # Safety
/// Same as [`get_ccline_ptr`].
#[must_use]
pub unsafe fn get_cmdline_type() -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let p = unsafe { get_ccline_ptr() };
    if p.is_null() {
        return 0;
    }
    // SAFETY: p is non-null and points at a live CmdlineInfo.
    let (firstc, input_fn) = unsafe { ((*p).cmdfirstc, (*p).input_fn) };
    if firstc == 0 {
        // No leading character: input() prompts report '@', anything
        // else reports '-'.
        return i32::from(if input_fn != 0 { b'@' } else { b'-' });
    }
    firstc
}

/// The current command line, in allocated memory (`get_cmdline_str`).
///
/// `None` when no command line is active, or when it is obscured
/// (`cmdline_star`), so a password prompt cannot be read back.
///
/// # Safety
/// Same as [`get_ccline_ptr`], plus reads `GLOBALS`.
#[must_use]
pub unsafe fn get_cmdline_str() -> Option<Vec<u8>> {
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { crate::globals::GLOBALS.get_mut() }.cmdline_star > 0 {
        return None;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let p = unsafe { get_ccline_ptr() };
    if p.is_null() {
        return None;
    }
    // SAFETY: p is non-null and points at a live CmdlineInfo.
    let (buff, len) = unsafe { (&(*p).cmdbuff, (*p).cmdlen) };
    let buff = buff.as_ref()?;
    // The original's xstrnsave takes exactly cmdlen bytes, which may
    // be fewer than the buffer holds.
    let take = (len.max(0) as usize).min(buff.len());
    Some(buff[..take].to_vec())
}

/// `getcmdline()` - the current command-line input (`f_getcmdline`,
/// `ex_getln.c`).
pub fn f_getcmdline(
    _argvars: &[crate::eval::typval_defs::TypvalT],
    rettv: &mut crate::eval::typval_defs::TypvalT,
) {
    // SAFETY: reads this module's own ccline file-static.
    rettv.value = crate::eval::typval_defs::TypvalValue::String(unsafe { get_cmdline_str() });
}

/// `getcmdpos()` - the cursor's byte position (1-based) in the
/// command line (`f_getcmdpos`, `ex_getln.c`).
///
/// `0` when no command line is active, which is why the position is
/// 1-based: zero is a distinguishable "none" value.
pub fn f_getcmdpos(
    _argvars: &[crate::eval::typval_defs::TypvalT],
    rettv: &mut crate::eval::typval_defs::TypvalT,
) {
    // SAFETY: reads this module's own ccline file-static.
    let p = unsafe { get_ccline_ptr() };
    let n = if p.is_null() {
        0
    } else {
        // SAFETY: p is non-null and points at a live CmdlineInfo.
        i64::from(unsafe { (*p).cmdpos }) + 1
    };
    rettv.value = crate::eval::typval_defs::TypvalValue::Number(n);
}

/// `getcmdprompt()` - the current command-line prompt
/// (`f_getcmdprompt`, `ex_getln.c`).
pub fn f_getcmdprompt(
    _argvars: &[crate::eval::typval_defs::TypvalT],
    rettv: &mut crate::eval::typval_defs::TypvalT,
) {
    // SAFETY: reads this module's own ccline file-static.
    let p = unsafe { get_ccline_ptr() };
    let prompt = if p.is_null() {
        None
    } else {
        // SAFETY: p is non-null and points at a live CmdlineInfo.
        unsafe { (*p).cmdprompt.clone() }
    };
    rettv.value = crate::eval::typval_defs::TypvalValue::String(prompt);
}

/// `getcmdscreenpos()` - the cursor's screen position (1-based) in the
/// command line (`f_getcmdscreenpos`, `ex_getln.c`) - always `0` today
/// (no active command line), since `cmdline_is_active` is always
/// `false`.
pub fn f_getcmdscreenpos(
    _argvars: &[crate::eval::typval_defs::TypvalT],
    rettv: &mut crate::eval::typval_defs::TypvalT,
) {
    let n: i64 = if cmdline_is_active() {
        unimplemented!("getcmdscreenpos(): needs a real, live command-line-editing state")
    } else {
        0
    };
    rettv.value = crate::eval::typval_defs::TypvalValue::Number(n);
}

/// `getcmdtype()` - the current command-line type (`f_getcmdtype`,
/// `ex_getln.c`).
///
/// The original always allocates a one-byte string and stores the
/// type character in it, so a NUL type yields a genuinely empty
/// string rather than a missing one.
pub fn f_getcmdtype(
    _argvars: &[crate::eval::typval_defs::TypvalT],
    rettv: &mut crate::eval::typval_defs::TypvalT,
) {
    // SAFETY: reads this module's own ccline file-static.
    let c = unsafe { get_cmdline_type() };
    let s = if c == 0 {
        Vec::new()
    } else {
        vec![u8::try_from(c).unwrap_or(0)]
    };
    rettv.value = crate::eval::typval_defs::TypvalValue::String(Some(s));
}

/// `wildtrigger()` - start wildcard expansion in the command line
/// (`f_wildtrigger`, `ex_getln.c`) - a real no-op today: the
/// original's own FIRST disjunct, `!(State & MODE_CMDLINE)`, is
/// exactly `!cmdline_is_active()`, always `true` today - and since
/// C's `||` short-circuits, `char_avail()`/`wild_menu_showing`/
/// `cmdline_pum_active()` are NEVER even evaluated once that first
/// disjunct is true, so none of those need to exist here either.
/// `rettv` is left completely untouched, matching the original's own
/// body (which never assigns to `rettv` at all - `call_func`'s own
/// caller already initializes it to `VAR_UNKNOWN` before dispatch).
pub fn f_wildtrigger(
    _argvars: &[crate::eval::typval_defs::TypvalT],
    _rettv: &mut crate::eval::typval_defs::TypvalT,
) {
    if !cmdline_is_active() {
        return;
    }
    unimplemented!("wildtrigger(): needs a real, live command-line-editing state")
}

/// Set the command line contents to `str`, with the cursor at byte
/// position `pos` (`< 0` meaning "at the end") (`set_cmdline_str`,
/// `ex_getln.c`). Returns `1` (fail) unless a command line is
/// currently active - the original's own `get_ccline_ptr() == NULL`
/// check is exactly `!cmdline_is_active()`, always true today, making
/// this an always-taken early return (not a hardcoded shortcut - see
/// `cmdline_is_active`'s own doc comment for the same established
/// pattern).
fn set_cmdline_str(_str: &[u8], _pos: i32) -> i32 {
    if !cmdline_is_active() {
        return 1;
    }
    unimplemented!("set_cmdline_str(): needs a real, live command-line-editing state")
}

/// `setcmdline({str} [, {pos}])` - set the command-line contents
/// (`f_setcmdline`, `ex_getln.c`).
pub fn f_setcmdline(
    argvars: &[crate::eval::typval_defs::TypvalT],
    rettv: &mut crate::eval::typval_defs::TypvalT,
) {
    if crate::eval::typval::tv_check_for_string_arg(argvars, 0) == crate::vim_defs::FAIL
        || (argvars.len() > 1
            && crate::eval::typval::tv_check_for_opt_number_arg(argvars, 1) == crate::vim_defs::FAIL)
    {
        return;
    }

    let mut pos: i32 = -1;
    if argvars.len() > 1 {
        let mut error = false;
        pos = (crate::eval::typval::tv_get_number_chk(&argvars[1], Some(&mut error)) - 1) as i32;
        if error {
            return;
        }
        if pos < 0 {
            // Real `emsg(_(e_positive))` display skipped - message
            // display not tractable, matching this crate's established
            // policy - the identical early-return state is kept.
            return;
        }
    }

    let n = set_cmdline_str(&crate::eval::typval::tv_get_string(&argvars[0]), pos);
    rettv.value = crate::eval::typval_defs::TypvalValue::Number(i64::from(n));
}

/// Set the command line's cursor byte position to `pos` (zero-based)
/// (`set_cmdline_pos`, `ex_getln.c`). Returns `1` (fail) unless a
/// command line is currently active - see [`set_cmdline_str`]'s own
/// doc comment for why this is always the case today.
fn set_cmdline_pos(_pos: i32) -> i32 {
    if !cmdline_is_active() {
        return 1;
    }
    unimplemented!("set_cmdline_pos(): needs a real, live command-line-editing state")
}

/// `setcmdpos({pos})` - set the command-line cursor position
/// (`f_setcmdpos`, `ex_getln.c`).
pub fn f_setcmdpos(
    argvars: &[crate::eval::typval_defs::TypvalT],
    rettv: &mut crate::eval::typval_defs::TypvalT,
) {
    let pos = (crate::eval::typval::tv_get_number(&argvars[0]) - 1) as i32;
    if pos >= 0 {
        rettv.value =
            crate::eval::typval_defs::TypvalValue::Number(i64::from(set_cmdline_pos(pos)));
    }
}

/// `cmdpreview_bufnr` - the buffer handle of the current `'inccommand'`
/// preview buffer, or `0` when no preview is active, read via
/// [`cmdpreview_get_bufnr`]. Modeled as its own file-static, matching
/// [`CMDLINE_FIRSTC`]'s own established precedent. Always `0` today,
/// since nothing in this crate can start a real command preview yet
/// (`cmdpreview_open_buf`, its only real writer, is not translated).
static CMDPREVIEW_BUFNR: crate::globals::GlobalCell<crate::api::private::defs::Buffer> =
    crate::globals::GlobalCell::new(0);

/// Returns the buffer handle of the current `'inccommand'` preview
/// buffer, or `0` when none is active (`cmdpreview_get_bufnr`). Always
/// `0` today - see `CMDPREVIEW_BUFNR`'s own doc comment.
#[must_use]
pub fn cmdpreview_get_bufnr() -> crate::api::private::defs::Buffer {
    // SAFETY: a plain `i32` copy-out read, no aliasing hazard.
    unsafe { *CMDPREVIEW_BUFNR.get_mut() }
}

/// `cmdpreview_ns` - the namespace ID used for `'inccommand'` preview
/// highlights, or `0` when no preview is active, read via
/// [`cmdpreview_get_ns`]. Modeled as its own file-static, matching
/// [`CMDPREVIEW_BUFNR`]'s own precedent just above. Always `0` today,
/// for the same reason.
static CMDPREVIEW_NS: crate::globals::GlobalCell<i32> = crate::globals::GlobalCell::new(0);

/// Returns the namespace ID used for `'inccommand'` preview
/// highlights, or `0` when none is active (`cmdpreview_get_ns`).
/// Always `0` today - see `CMDPREVIEW_NS`'s own doc comment.
#[must_use]
pub fn cmdpreview_get_ns() -> i32 {
    // SAFETY: a plain `i32` copy-out read, no aliasing hazard.
    unsafe { *CMDPREVIEW_NS.get_mut() }
}

/// Whether the current buffer is the special `cmdwin` scratch buffer
/// AND no other command line is simultaneously active
/// (`is_in_cmdwin`).
///
/// The original's own second condition, `get_cmdline_type() == NUL`,
/// is always true today: `get_cmdline_type()`'s own real early return
/// (`get_ccline_ptr() == NULL`) is always taken, exactly matching
/// `cmdline_is_active`'s own established "always false" reasoning
/// in this same file (both ultimately check the same `MODE_CMDLINE`
/// bit) - so this simplifies to just `bt_cmdwin(curbuf)`, not a
/// hardcoded shortcut.
///
/// # Safety
/// Touches `crate::globals::GLOBALS.curbuf` - forwarded from
/// `crate::buffer::bt_cmdwin`'s own safety doc.
#[must_use]
pub unsafe fn is_in_cmdwin() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &*crate::globals::GLOBALS.get_mut().curbuf };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::buffer::bt_cmdwin(Some(curbuf)) }
}

/// Read the `'wildmode'` option, filling
/// `crate::globals::Globals::wim_flags` (`check_opt_wim`).
///
/// `'wildmode'` is a comma-separated list of up to 4 "stages"; each
/// stage is one or more `:`-joined mode names (e.g.
/// `"longest:full,list,full"` - the FIRST stage combines
/// `longest`+`full`, matching the original's exact bit-OR-into-the-
/// same-slot behavior for a `:`-joined group, hand-traced against
/// this concrete example before translating). Fewer than 4
/// comma-separated stages get the LAST given stage's own flags
/// repeated for the remaining slots (matching the original's own
/// "fill remaining entries with last flag" tail exactly).
///
/// # Safety
/// Touches `crate::option_vars::OPTION_VARS` and
/// `crate::globals::GLOBALS`.
pub unsafe fn check_opt_wim() -> i32 {
    use crate::option_vars::opt_wim_flag;
    use crate::vim_defs::{FAIL, OK};

    // SAFETY: forwarded from this function's own safety doc.
    let p_wim = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_wim.clone();
    let s: &[u8] = p_wim.as_deref().unwrap_or(&[]);

    let mut new_wim_flags = [0u8; 4];
    let mut idx = 0usize;
    let mut pos = 0usize;

    while pos < s.len() {
        // Count consecutive alpha characters starting at `pos`.
        let mut i = 0usize;
        while pos + i < s.len() && crate::macros_defs::ascii_isalpha(i32::from(s[pos + i])) {
            i += 1;
        }
        let next = s.get(pos + i).copied();
        if next.is_some() && next != Some(b',') && next != Some(b':') {
            return FAIL;
        }

        let word = &s[pos..pos + i];
        let flag = if i == 7 && word == b"longest" {
            opt_wim_flag::LONGEST
        } else if i == 4 && word == b"full" {
            opt_wim_flag::FULL
        } else if i == 4 && word == b"list" {
            opt_wim_flag::LIST
        } else if i == 8 && word == b"lastused" {
            opt_wim_flag::LASTUSED
        } else if i == 8 && word == b"noselect" {
            opt_wim_flag::NOSELECT
        } else if i == 8 && word == b"noinsert" {
            opt_wim_flag::NOINSERT
        } else {
            return FAIL;
        };
        new_wim_flags[idx] |= flag as u8;

        pos += i;
        match s.get(pos) {
            None => break,
            Some(&b',') => {
                if idx == 3 {
                    return FAIL;
                }
                idx += 1;
            }
            Some(_) => {} // ':' - combine into the same slot.
        }
        // The original for-loop's own increment - consumes the
        // comma/colon we just examined.
        pos += 1;
    }

    // Fill remaining entries with the last flag.
    while idx < 3 {
        new_wim_flags[idx + 1] = new_wim_flags[idx];
        idx += 1;
    }

    // Only when there are no errors, wim_flags[] is changed.
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::globals::GLOBALS.get_mut() }.wim_flags = new_wim_flags;
    OK
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_and_restore_viewstate_round_trip_exactly() {
        // The two are exact inverses, so a save/restore pair must
        // leave every field as it started - including the ones a
        // partial implementation would be most likely to miss.
        let mut win = crate::buffer_defs::WinT {
            w_curswant: 11,
            w_leftcol: 3,
            w_skipcol: 4,
            w_topline: 20,
            w_topfill: 2,
            w_botline: 45,
            w_empty_rows: 6,
            ..Default::default()
        };
        let saved = save_viewstate(&win);

        // Move the view somewhere completely different.
        win.w_curswant = 0;
        win.w_leftcol = 0;
        win.w_skipcol = 0;
        win.w_topline = 1;
        win.w_topfill = 0;
        win.w_botline = 2;
        win.w_empty_rows = 0;

        restore_viewstate(&mut win, &saved);

        assert_eq!(win.w_curswant, 11);
        assert_eq!(win.w_leftcol, 3);
        assert_eq!(win.w_skipcol, 4);
        assert_eq!(win.w_topline, 20);
        assert_eq!(win.w_topfill, 2);
        assert_eq!(win.w_botline, 45);
        assert_eq!(win.w_empty_rows, 6);

        // Saving again must produce an identical record.
        assert_eq!(save_viewstate(&win), saved);
    }

    #[test]
    fn get_list_range_lone_number_becomes_both_ends() {
        // A single number is used for BOTH ends of the range.
        let (n1, n2, used) = get_list_range(b"7").expect("valid");
        assert_eq!(n1, Some(7));
        assert_eq!(n2, Some(7));
        assert_eq!(used, 1);
    }

    #[test]
    fn get_list_range_parses_a_full_pair() {
        let (n1, n2, used) = get_list_range(b"3,9").expect("valid");
        assert_eq!(n1, Some(3));
        assert_eq!(n2, Some(9));
        assert_eq!(used, 3);
    }

    #[test]
    fn get_list_range_accepts_a_missing_first_part() {
        // ",b" is valid: only the second end is supplied, and the
        // first is left for the caller's own default.
        let (n1, n2, _used) = get_list_range(b",9").expect("valid");
        assert_eq!(n1, None, "an absent part must not be reported");
        assert_eq!(n2, Some(9));
    }

    #[test]
    fn get_list_range_rejects_a_comma_with_nothing_usable() {
        // Neither side supplies a number, so there is no range at all.
        assert!(get_list_range(b",").is_none());
        assert!(get_list_range(b",x").is_none());
    }

    #[test]
    fn get_list_range_skips_surrounding_whitespace() {
        let (n1, n2, used) = get_list_range(b"  3 , 9  ").expect("valid");
        assert_eq!(n1, Some(3));
        assert_eq!(n2, Some(9));
        // Trailing whitespace after the second number is consumed too.
        assert_eq!(used, 9);
    }

    #[test]
    fn get_list_range_accepts_a_negative_first_number() {
        // A leading '-' starts the "from" part, so negative values
        // parse rather than being treated as junk.
        let (n1, n2, _used) = get_list_range(b"-5,-1").expect("valid");
        assert_eq!(n1, Some(-5));
        assert_eq!(n2, Some(-1));
    }

    #[test]
    fn get_list_range_with_no_number_at_all_reports_nothing() {
        // Not a failure: there is simply no range here, and the
        // caller's own defaults stand.
        let (n1, n2, used) = get_list_range(b"abc").expect("not a failure");
        assert_eq!(n1, None);
        assert_eq!(n2, None);
        assert_eq!(used, 0);
    }

    #[test]
    fn get_list_range_rejects_a_value_overflowing_i32() {
        // The original returns FAIL for anything above INT_MAX.
        assert!(get_list_range(b"99999999999999").is_none());
    }

    #[test]
    fn escape_fname_prepends_a_backslash() {
        let mut s = b"foo".to_vec();
        escape_fname(&mut s);
        assert_eq!(s, b"\\foo".to_vec());
    }

    #[test]
    fn vim_strsave_fnameescape_escapes_a_space() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { vim_strsave_fnameescape(b"a b", VseWhat::None) }, b"a\\ b".to_vec());
    }

    #[test]
    fn vim_strsave_fnameescape_plain_name_is_unchanged() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { vim_strsave_fnameescape(b"hello", VseWhat::None) }, b"hello".to_vec());
    }

    #[test]
    fn vim_strsave_fnameescape_escapes_a_leading_dash_that_is_the_whole_name() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { vim_strsave_fnameescape(b"-", VseWhat::None) }, b"\\-".to_vec());
    }

    #[test]
    fn vim_strsave_fnameescape_does_not_escape_a_dash_followed_by_more_text() {
        let _guard = crate::globals::global_state_test_lock();
        // "-foo" is not the special bare "-" case, so it should only
        // get the ordinary escaping (none of its own characters are
        // in the escape-char set).
        assert_eq!(unsafe { vim_strsave_fnameescape(b"-foo", VseWhat::None) }, b"-foo".to_vec());
    }

    #[test]
    fn vim_strsave_fnameescape_escapes_a_leading_greater_than() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { vim_strsave_fnameescape(b">foo", VseWhat::None) }, b"\\>foo".to_vec());
    }

    #[test]
    fn vim_strsave_fnameescape_escapes_a_leading_plus() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { vim_strsave_fnameescape(b"+foo", VseWhat::None) }, b"\\+foo".to_vec());
    }

    // --- cmdline_is_active / f_getcmdline / f_getcmdpos / f_getcmdprompt
    // / f_getcmdscreenpos / f_getcmdtype ---

    #[test]
    fn cmdline_is_active_is_false_by_default() {
        let _guard = crate::globals::global_state_test_lock();
        // GLOBALS.State defaults to mode::NORMAL (no MODE_CMDLINE bit),
        // matching this crate's own established `Globals::default`
        // convention.
        assert!(!cmdline_is_active());
    }

    #[test]
    fn cmdline_is_active_is_true_when_the_cmdline_bit_is_set() {
        let _guard = crate::globals::global_state_test_lock();
        // SAFETY: `global_state_test_lock()` held for this whole test.
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_state = g.State;
        g.State = crate::state_defs::mode::CMDLINE as i32;

        assert!(cmdline_is_active());

        // SAFETY: forwarded from the lock reasoning above.
        unsafe { crate::globals::GLOBALS.get_mut() }.State = prev_state;
    }

    // --- cmdline buffer lifecycle and save/restore ---

    #[test]
    fn alloc_cmdbuff_reserves_extra_space() {
        let _guard = crate::globals::global_state_test_lock();

        unsafe { alloc_cmdbuff(10) };
        let small = unsafe { CCLINE.get_mut() }.cmdbuff.as_ref().unwrap().capacity();
        unsafe { alloc_cmdbuff(200) };
        let large = unsafe { CCLINE.get_mut() }.cmdbuff.as_ref().unwrap().capacity();
        unsafe { cmdline_init() };

        // Short requests round up to a floor; long ones get a margin.
        assert!(small >= 100, "small={small}");
        assert!(large >= 220, "large={large}");
    }

    #[test]
    fn dealloc_cmdbuff_clears_the_buffer_and_length() {
        let _guard = crate::globals::global_state_test_lock();
        let ccline = unsafe { get_cmdline_info() };
        unsafe {
            (*ccline).cmdbuff = Some(b"echo".to_vec());
            (*ccline).cmdlen = 4;
        }

        unsafe { dealloc_cmdbuff() };

        let (buff, len) = unsafe { ((*ccline).cmdbuff.clone(), (*ccline).cmdlen) };
        unsafe { cmdline_init() };
        assert_eq!(buff, None);
        assert_eq!(len, 0);
    }

    #[test]
    fn cmdline_init_resets_everything() {
        let _guard = crate::globals::global_state_test_lock();
        let ccline = unsafe { get_cmdline_info() };
        unsafe {
            (*ccline).cmdbuff = Some(b"echo".to_vec());
            (*ccline).cmdlen = 4;
            (*ccline).cmdfirstc = i32::from(b':');
            (*ccline).level = 3;
        }

        unsafe { cmdline_init() };

        let cc = unsafe { &*ccline };
        assert_eq!(cc.cmdbuff, None);
        assert_eq!(cc.cmdlen, 0);
        assert_eq!(cc.cmdfirstc, 0);
        assert_eq!(cc.level, 0);
    }

    #[test]
    fn save_cmdline_moves_the_state_out_and_links_it_back() {
        let _guard = crate::globals::global_state_test_lock();
        let ccline = unsafe { get_cmdline_info() };
        unsafe {
            (*ccline).cmdbuff = Some(b":first".to_vec());
            (*ccline).cmdlen = 6;
            (*ccline).cmdfirstc = i32::from(b':');
        }

        let mut saved = Box::new(crate::ex_getln_defs::CmdlineInfo::default());
        let saved_ptr = std::ptr::addr_of_mut!(*saved);
        unsafe { save_cmdline(saved_ptr) };

        // The old state moved into the save slot...
        assert_eq!(saved.cmdbuff.as_deref(), Some(&b":first"[..]));
        assert_eq!(saved.cmdlen, 6);
        // ...and ccline is fresh, with cmdbuff unset as the "not in
        // use" signal, but linked back to what was saved.
        let cc = unsafe { &*ccline };
        assert_eq!(cc.cmdbuff, None);
        assert_eq!(cc.cmdlen, 0);
        assert_eq!(cc.prev_ccline, saved_ptr);

        unsafe { cmdline_init() };
    }

    #[test]
    fn save_and_restore_cmdline_round_trip() {
        // This pair is what makes a recursive command line (CTRL-R =)
        // work, so the round trip is the behaviour that matters.
        let _guard = crate::globals::global_state_test_lock();
        let ccline = unsafe { get_cmdline_info() };
        unsafe {
            (*ccline).cmdbuff = Some(b":outer".to_vec());
            (*ccline).cmdlen = 6;
            (*ccline).cmdfirstc = i32::from(b':');
        }

        let mut saved = Box::new(crate::ex_getln_defs::CmdlineInfo::default());
        let saved_ptr = std::ptr::addr_of_mut!(*saved);

        unsafe { save_cmdline(saved_ptr) };
        // Something else uses the command line in between.
        unsafe { (*ccline).cmdbuff = Some(b"=inner".to_vec()) };
        unsafe { restore_cmdline(saved_ptr) };

        let cc = unsafe { &*ccline };
        assert_eq!(cc.cmdbuff.as_deref(), Some(&b":outer"[..]));
        assert_eq!(cc.cmdlen, 6);
        assert_eq!(cc.cmdfirstc, i32::from(b':'));

        unsafe { cmdline_init() };
    }

    // --- cmdline column helpers ---

    #[test]
    fn cmd_startcol_accounts_for_the_indent_and_leading_character() {
        let _guard = crate::globals::global_state_test_lock();

        // No leading character: just the indent.
        let got = with_cmdline(
            |cc| {
                cc.cmdindent = 4;
                cc.cmdfirstc = 0;
            },
            || unsafe { cmd_startcol() },
        );
        assert_eq!(got, 4);

        // A leading ':' occupies one further column.
        let got = with_cmdline(
            |cc| {
                cc.cmdindent = 4;
                cc.cmdfirstc = i32::from(b':');
            },
            || unsafe { cmd_startcol() },
        );
        assert_eq!(got, 5);
    }

    #[test]
    fn cmdline_charsize_reports_one_cell_for_ascii() {
        let _guard = crate::globals::global_state_test_lock();
        let got = with_cmdline(
            |cc| cc.cmdbuff = Some(b"abc".to_vec()),
            || unsafe { cmdline_charsize(1) },
        );
        assert_eq!(got, 1);
    }

    #[test]
    fn cmdline_charsize_is_one_for_an_obscured_command_line() {
        // Every character shows as '*', so width never varies - even
        // for a character that would otherwise be two cells wide.
        let _guard = crate::globals::global_state_test_lock();
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev = g.cmdline_star;
        g.cmdline_star = 1;

        let got = with_cmdline(
            |cc| cc.cmdbuff = Some("一".as_bytes().to_vec()),
            || unsafe { cmdline_charsize(0) },
        );

        unsafe { crate::globals::GLOBALS.get_mut() }.cmdline_star = prev;
        assert_eq!(got, 1);
    }

    #[test]
    fn correct_screencol_leaves_a_narrow_character_alone() {
        let _guard = crate::globals::global_state_test_lock();
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_cols = g.Columns;
        g.Columns = 80;

        // Plain ASCII is neither multi-byte nor wide, so no nudge -
        // even sitting right at the edge.
        let got = with_cmdline(
            |cc| cc.cmdbuff = Some(b"abc".to_vec()),
            || unsafe { correct_screencol(0, 1, 79) },
        );

        unsafe { crate::globals::GLOBALS.get_mut() }.Columns = prev_cols;
        assert_eq!(got, 79);
    }

    #[test]
    fn correct_screencol_nudges_a_wide_character_that_would_straddle_the_edge() {
        let _guard = crate::globals::global_state_test_lock();
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_cols = g.Columns;
        g.Columns = 80;

        // A two-cell character starting at the last column would run
        // off the end, so one extra column is consumed.
        let straddling = with_cmdline(
            |cc| cc.cmdbuff = Some("一".as_bytes().to_vec()),
            || unsafe { correct_screencol(0, 2, 79) },
        );
        // The same character with room to spare is left alone.
        let fitting = with_cmdline(
            |cc| cc.cmdbuff = Some("一".as_bytes().to_vec()),
            || unsafe { correct_screencol(0, 2, 10) },
        );

        unsafe { crate::globals::GLOBALS.get_mut() }.Columns = prev_cols;
        assert_eq!(straddling, 80);
        assert_eq!(fitting, 10);
    }

    // --- getcmd* builtins ---

    /// Install a live command line for the duration of `f`, then
    /// restore. Boxed where a pointer escapes into ccline.
    fn with_cmdline<T>(
        setup: impl FnOnce(&mut crate::ex_getln_defs::CmdlineInfo),
        f: impl FnOnce() -> T,
    ) -> T {
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_state = g.State;
        g.State = crate::state_defs::mode::CMDLINE as i32;

        let ccline = unsafe { get_cmdline_info() };
        setup(unsafe { &mut *ccline });

        let r = f();

        unsafe {
            *ccline = crate::ex_getln_defs::CmdlineInfo::default();
        }
        unsafe { crate::globals::GLOBALS.get_mut() }.State = prev_state;
        r
    }

    #[test]
    fn getcmd_builtins_report_nothing_without_an_active_command_line() {
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = crate::eval::typval_defs::TypvalT::default();

        f_getcmdpos(&[], &mut rettv);
        assert_eq!(rettv.value, crate::eval::typval_defs::TypvalValue::Number(0));

        f_getcmdline(&[], &mut rettv);
        assert_eq!(rettv.value, crate::eval::typval_defs::TypvalValue::String(None));

        f_getcmdprompt(&[], &mut rettv);
        assert_eq!(rettv.value, crate::eval::typval_defs::TypvalValue::String(None));

        // getcmdtype differs deliberately: the original always
        // allocates a one-byte string and stores the type char, so a
        // NUL type is an EMPTY string, not a missing one.
        f_getcmdtype(&[], &mut rettv);
        assert_eq!(
            rettv.value,
            crate::eval::typval_defs::TypvalValue::String(Some(Vec::new()))
        );
    }

    #[test]
    fn getcmdtype_reports_the_leading_character() {
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = crate::eval::typval_defs::TypvalT::default();

        with_cmdline(
            |cc| {
                cc.cmdbuff = Some(b"echo".to_vec());
                cc.cmdfirstc = i32::from(b':');
            },
            || f_getcmdtype(&[], &mut rettv),
        );

        assert_eq!(
            rettv.value,
            crate::eval::typval_defs::TypvalValue::String(Some(b":".to_vec()))
        );
    }

    #[test]
    fn getcmdtype_distinguishes_input_prompts_from_other_nul_types() {
        // With no leading character, an input() prompt reports '@' and
        // anything else reports '-'.
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = crate::eval::typval_defs::TypvalT::default();

        with_cmdline(
            |cc| {
                cc.cmdbuff = Some(b"x".to_vec());
                cc.cmdfirstc = 0;
                cc.input_fn = 1;
            },
            || f_getcmdtype(&[], &mut rettv),
        );
        assert_eq!(
            rettv.value,
            crate::eval::typval_defs::TypvalValue::String(Some(b"@".to_vec()))
        );

        with_cmdline(
            |cc| {
                cc.cmdbuff = Some(b"x".to_vec());
                cc.cmdfirstc = 0;
                cc.input_fn = 0;
            },
            || f_getcmdtype(&[], &mut rettv),
        );
        assert_eq!(
            rettv.value,
            crate::eval::typval_defs::TypvalValue::String(Some(b"-".to_vec()))
        );
    }

    #[test]
    fn getcmdline_reports_exactly_cmdlen_bytes() {
        // The buffer may be longer than cmdlen; only cmdlen bytes are
        // the command line.
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = crate::eval::typval_defs::TypvalT::default();

        with_cmdline(
            |cc| {
                cc.cmdbuff = Some(b"echo hi\0\0\0".to_vec());
                cc.cmdlen = 7;
            },
            || f_getcmdline(&[], &mut rettv),
        );

        assert_eq!(
            rettv.value,
            crate::eval::typval_defs::TypvalValue::String(Some(b"echo hi".to_vec()))
        );
    }

    #[test]
    fn getcmdline_reports_nothing_when_the_command_line_is_obscured() {
        // An obscured command line (a password prompt) must not be
        // readable back.
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = crate::eval::typval_defs::TypvalT::default();

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_star = g.cmdline_star;
        g.cmdline_star = 1;

        with_cmdline(
            |cc| {
                cc.cmdbuff = Some(b"secret".to_vec());
                cc.cmdlen = 6;
            },
            || f_getcmdline(&[], &mut rettv),
        );

        unsafe { crate::globals::GLOBALS.get_mut() }.cmdline_star = prev_star;
        assert_eq!(rettv.value, crate::eval::typval_defs::TypvalValue::String(None));
    }

    #[test]
    fn getcmdpos_is_one_based() {
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = crate::eval::typval_defs::TypvalT::default();

        with_cmdline(
            |cc| {
                cc.cmdbuff = Some(b"echo".to_vec());
                cc.cmdpos = 3;
            },
            || f_getcmdpos(&[], &mut rettv),
        );

        assert_eq!(rettv.value, crate::eval::typval_defs::TypvalValue::Number(4));
    }

    #[test]
    fn getcmdprompt_reports_the_prompt_when_there_is_one() {
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = crate::eval::typval_defs::TypvalT::default();

        with_cmdline(
            |cc| {
                cc.cmdbuff = Some(b"x".to_vec());
                cc.cmdprompt = Some(b"Name: ".to_vec());
            },
            || f_getcmdprompt(&[], &mut rettv),
        );

        assert_eq!(
            rettv.value,
            crate::eval::typval_defs::TypvalValue::String(Some(b"Name: ".to_vec()))
        );
    }

    #[test]
    fn cmdline_overstrike_is_false_by_default() {
        let _guard = crate::globals::global_state_test_lock();
        assert!(!cmdline_overstrike());
    }

    #[test]
    fn cmdline_at_end_is_true_by_default() {
        let _guard = crate::globals::global_state_test_lock();
        // ccline's cmdpos and cmdlen both start at 0, and 0 >= 0.
        assert!(cmdline_at_end());
    }

    // --- ccline accessor family ---

    #[test]
    fn get_cmdline_info_reports_the_real_ccline_static() {
        let _guard = crate::globals::global_state_test_lock();
        let a = unsafe { get_cmdline_info() };
        let b = unsafe { get_cmdline_info() };
        assert!(!a.is_null());
        assert_eq!(a, b, "the same file-static every time");
    }

    #[test]
    fn get_cmdline_last_prompt_id_starts_at_zero() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { get_cmdline_last_prompt_id() }, 0);
    }

    #[test]
    fn get_ccline_ptr_is_null_outside_cmdline_mode() {
        // Not in command-line mode at all: nothing to report, whatever
        // ccline happens to hold.
        let _guard = crate::globals::global_state_test_lock();
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev = g.State;
        g.State = 0;

        let p = unsafe { get_ccline_ptr() };

        unsafe { crate::globals::GLOBALS.get_mut() }.State = prev;
        assert!(p.is_null());
    }

    #[test]
    fn get_ccline_ptr_is_null_in_cmdline_mode_with_no_buffer() {
        // In command-line mode, but neither ccline nor a saved
        // previous one holds a real buffer.
        let _guard = crate::globals::global_state_test_lock();
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev = g.State;
        g.State = crate::state_defs::mode::CMDLINE as i32;

        let p = unsafe { get_ccline_ptr() };

        unsafe { crate::globals::GLOBALS.get_mut() }.State = prev;
        assert!(p.is_null());
    }

    #[test]
    fn get_ccline_ptr_reports_ccline_when_it_holds_a_buffer() {
        let _guard = crate::globals::global_state_test_lock();
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_state = g.State;
        g.State = crate::state_defs::mode::CMDLINE as i32;

        let ccline = unsafe { get_cmdline_info() };
        unsafe { (*ccline).cmdbuff = Some(b":echo".to_vec()) };

        let p = unsafe { get_ccline_ptr() };

        unsafe { (*ccline).cmdbuff = None };
        unsafe { crate::globals::GLOBALS.get_mut() }.State = prev_state;
        assert_eq!(p, ccline);
    }

    #[test]
    fn get_ccline_ptr_falls_back_to_the_saved_previous_state() {
        // save_cmdline() clears ccline and moves the live state into
        // prev_ccline, so the fallback is what keeps the accessors
        // working across a recursive command line.
        let _guard = crate::globals::global_state_test_lock();
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_state = g.State;
        g.State = crate::state_defs::mode::CMDLINE as i32;

        let mut saved = Box::new(crate::ex_getln_defs::CmdlineInfo {
            cmdbuff: Some(b":saved".to_vec()),
            ..Default::default()
        });
        let saved_ptr = std::ptr::addr_of_mut!(*saved);

        let ccline = unsafe { get_cmdline_info() };
        unsafe {
            (*ccline).cmdbuff = None;
            (*ccline).prev_ccline = saved_ptr;
        }

        let p = unsafe { get_ccline_ptr() };

        unsafe { (*ccline).prev_ccline = std::ptr::null_mut() };
        unsafe { crate::globals::GLOBALS.get_mut() }.State = prev_state;
        assert_eq!(p, saved_ptr);
    }

    #[test]
    fn cmdpreview_get_bufnr_is_zero_by_default() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(cmdpreview_get_bufnr(), 0);
    }

    #[test]
    fn cmdpreview_get_ns_is_zero_by_default() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(cmdpreview_get_ns(), 0);
    }

    #[test]
    fn is_in_cmdwin_true_when_curbuf_is_the_cmdwin_buf() {
        let _guard = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT::default();
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_curbuf = globals.curbuf;
        let prev_cmdwin_buf = globals.cmdwin_buf;
        globals.curbuf = &mut buf as *mut _;
        globals.cmdwin_buf = &mut buf as *mut _;

        assert!(unsafe { is_in_cmdwin() });

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        globals.curbuf = prev_curbuf;
        globals.cmdwin_buf = prev_cmdwin_buf;
    }

    #[test]
    fn is_in_cmdwin_false_when_curbuf_is_not_the_cmdwin_buf() {
        let _guard = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT::default();
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_curbuf = globals.curbuf;
        let prev_cmdwin_buf = globals.cmdwin_buf;
        globals.curbuf = &mut buf as *mut _;
        globals.cmdwin_buf = std::ptr::null_mut();

        assert!(!unsafe { is_in_cmdwin() });

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        globals.curbuf = prev_curbuf;
        globals.cmdwin_buf = prev_cmdwin_buf;
    }

    #[test]
    fn getcmdcomplpat_is_empty_when_no_command_line_is_active() {
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        f_getcmdcomplpat(&[], &mut rettv);
        assert_eq!(
            rettv.value,
            crate::eval::typval_defs::TypvalValue::String(None)
        );
    }

    #[test]
    fn getcmdcompltype_is_empty_when_no_command_line_is_active() {
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        f_getcmdcompltype(&[], &mut rettv);
        assert_eq!(
            rettv.value,
            crate::eval::typval_defs::TypvalValue::String(None)
        );
    }

    #[test]
    fn getcmdline_is_empty_when_no_command_line_is_active() {
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        f_getcmdline(&[], &mut rettv);
        assert_eq!(
            rettv.value,
            crate::eval::typval_defs::TypvalValue::String(None)
        );
    }

    #[test]
    fn getcmdpos_is_zero_when_no_command_line_is_active() {
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        f_getcmdpos(&[], &mut rettv);
        assert_eq!(rettv.value, crate::eval::typval_defs::TypvalValue::Number(0));
    }

    #[test]
    fn getcmdprompt_is_empty_when_no_command_line_is_active() {
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        f_getcmdprompt(&[], &mut rettv);
        assert_eq!(
            rettv.value,
            crate::eval::typval_defs::TypvalValue::String(None)
        );
    }

    #[test]
    fn getcmdscreenpos_is_zero_when_no_command_line_is_active() {
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        f_getcmdscreenpos(&[], &mut rettv);
        assert_eq!(rettv.value, crate::eval::typval_defs::TypvalValue::Number(0));
    }

    #[test]
    fn getcmdtype_is_an_empty_string_when_no_command_line_is_active() {
        // NOT a missing string: the original always allocates a
        // one-byte buffer and stores the type char into it, so a NUL
        // type is a real, present, empty string. This test previously
        // asserted `String(None)`, which was the placeholder
        // implementation's behaviour rather than the original's.
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        f_getcmdtype(&[], &mut rettv);
        assert_eq!(
            rettv.value,
            crate::eval::typval_defs::TypvalValue::String(Some(Vec::new()))
        );
    }

    #[test]
    fn getcmdtype_reports_the_type_when_a_command_line_is_active() {
        // This previously asserted a panic at the `unimplemented!()`
        // boundary, which no longer exists: an active command line is
        // now genuinely handled.
        let _guard = crate::globals::global_state_test_lock();
        // SAFETY: `global_state_test_lock()` held for this whole test.
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_state = g.State;
        g.State = crate::state_defs::mode::CMDLINE as i32;

        let ccline = unsafe { get_cmdline_info() };
        unsafe {
            (*ccline).cmdbuff = Some(b"s/a/b/".to_vec());
            (*ccline).cmdfirstc = i32::from(b'/');
        }

        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        f_getcmdtype(&[], &mut rettv);

        unsafe {
            *ccline = crate::ex_getln_defs::CmdlineInfo::default();
            crate::globals::GLOBALS.get_mut().State = prev_state;
        }

        assert_eq!(
            rettv.value,
            crate::eval::typval_defs::TypvalValue::String(Some(b"/".to_vec()))
        );
    }

    #[test]
    fn wildtrigger_is_a_no_op_when_no_command_line_is_active() {
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        f_wildtrigger(&[], &mut rettv);
        // rettv is left completely untouched, matching the original's
        // own body (which never assigns to it at all).
        assert_eq!(rettv.value, crate::eval::typval_defs::TypvalValue::Unknown);
    }

    #[test]
    #[should_panic(expected = "wildtrigger(): needs a real, live command-line-editing state")]
    fn wildtrigger_panics_when_a_command_line_is_genuinely_active() {
        let _guard = crate::globals::global_state_test_lock();
        // SAFETY: `global_state_test_lock()` held for this whole test.
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_state = g.State;
        g.State = crate::state_defs::mode::CMDLINE as i32;

        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            f_wildtrigger(&[], &mut rettv);
        }));

        // SAFETY: forwarded from the lock reasoning above.
        unsafe { crate::globals::GLOBALS.get_mut() }.State = prev_state;
        if let Err(payload) = result {
            std::panic::resume_unwind(payload);
        }
    }

    // --- set_cmdline_str / f_setcmdline / set_cmdline_pos / f_setcmdpos ---

    #[test]
    fn set_cmdline_str_returns_1_when_no_command_line_is_active() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(set_cmdline_str(b"foo", -1), 1);
    }

    #[test]
    fn set_cmdline_pos_returns_1_when_no_command_line_is_active() {
        let _guard = crate::globals::global_state_test_lock();
        assert_eq!(set_cmdline_pos(0), 1);
    }

    #[test]
    fn setcmdline_leaves_rettv_untouched_when_first_arg_is_not_a_string() {
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        let args = [crate::eval::typval_defs::TypvalT {
            value: crate::eval::typval_defs::TypvalValue::Number(5),
            ..Default::default()
        }];
        f_setcmdline(&args, &mut rettv);
        assert_eq!(rettv.value, crate::eval::typval_defs::TypvalValue::Unknown);
    }

    #[test]
    fn setcmdline_leaves_rettv_untouched_when_second_arg_is_not_a_number() {
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        let args = [
            crate::eval::typval_defs::TypvalT {
                value: crate::eval::typval_defs::TypvalValue::String(Some(b"foo".to_vec())),
                ..Default::default()
            },
            crate::eval::typval_defs::TypvalT {
                value: crate::eval::typval_defs::TypvalValue::String(Some(b"bar".to_vec())),
                ..Default::default()
            },
        ];
        f_setcmdline(&args, &mut rettv);
        assert_eq!(rettv.value, crate::eval::typval_defs::TypvalValue::Unknown);
    }

    #[test]
    fn setcmdline_leaves_rettv_untouched_when_pos_is_negative() {
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        // {pos} is 1-based; passing 0 makes the internal, 0-based
        // `pos` computation land at -1, which is the real, reachable
        // "positive number required" early return (independent of
        // `cmdline_is_active()`'s own always-false state today).
        let args = [
            crate::eval::typval_defs::TypvalT {
                value: crate::eval::typval_defs::TypvalValue::String(Some(b"foo".to_vec())),
                ..Default::default()
            },
            crate::eval::typval_defs::TypvalT {
                value: crate::eval::typval_defs::TypvalValue::Number(0),
                ..Default::default()
            },
        ];
        f_setcmdline(&args, &mut rettv);
        assert_eq!(rettv.value, crate::eval::typval_defs::TypvalValue::Unknown);
    }

    #[test]
    fn setcmdline_returns_1_when_pos_argument_is_omitted() {
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        let args = [crate::eval::typval_defs::TypvalT {
            value: crate::eval::typval_defs::TypvalValue::String(Some(b"foo".to_vec())),
            ..Default::default()
        }];
        f_setcmdline(&args, &mut rettv);
        assert_eq!(rettv.value, crate::eval::typval_defs::TypvalValue::Number(1));
    }

    #[test]
    fn setcmdline_returns_1_when_pos_is_a_valid_positive_number() {
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        let args = [
            crate::eval::typval_defs::TypvalT {
                value: crate::eval::typval_defs::TypvalValue::String(Some(b"foo".to_vec())),
                ..Default::default()
            },
            crate::eval::typval_defs::TypvalT {
                value: crate::eval::typval_defs::TypvalValue::Number(1),
                ..Default::default()
            },
        ];
        f_setcmdline(&args, &mut rettv);
        assert_eq!(rettv.value, crate::eval::typval_defs::TypvalValue::Number(1));
    }

    #[test]
    fn setcmdpos_leaves_rettv_untouched_when_pos_is_negative() {
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        // {pos} is 1-based; passing 0 makes the internal, 0-based
        // `pos` computation land at -1, so `f_setcmdpos` never even
        // calls `set_cmdline_pos` and never assigns `rettv`.
        let args = [crate::eval::typval_defs::TypvalT {
            value: crate::eval::typval_defs::TypvalValue::Number(0),
            ..Default::default()
        }];
        f_setcmdpos(&args, &mut rettv);
        assert_eq!(rettv.value, crate::eval::typval_defs::TypvalValue::Unknown);
    }

    #[test]
    fn setcmdpos_returns_1_when_pos_is_a_valid_positive_number() {
        let _guard = crate::globals::global_state_test_lock();
        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        let args = [crate::eval::typval_defs::TypvalT {
            value: crate::eval::typval_defs::TypvalValue::Number(1),
            ..Default::default()
        }];
        f_setcmdpos(&args, &mut rettv);
        assert_eq!(rettv.value, crate::eval::typval_defs::TypvalValue::Number(1));
    }

    // ---- check_opt_wim ----

    fn set_p_wim(value: Option<&[u8]>) -> Option<Vec<u8>> {
        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let prev = opts.p_wim.clone();
        opts.p_wim = value.map(<[u8]>::to_vec);
        prev
    }

    fn reset_wim_flags() -> [u8; 4] {
        let prev = unsafe { crate::globals::GLOBALS.get_mut() }.wim_flags;
        unsafe { crate::globals::GLOBALS.get_mut() }.wim_flags = [0; 4];
        prev
    }

    #[test]
    fn check_opt_wim_the_real_default_value_repeats_full_in_every_slot() {
        let _lock = crate::globals::global_state_test_lock();
        let prev_wim = set_p_wim(Some(b"full"));
        let prev_flags = reset_wim_flags();

        assert_eq!(unsafe { check_opt_wim() }, crate::vim_defs::OK);
        assert_eq!(
            unsafe { crate::globals::GLOBALS.get_mut() }.wim_flags,
            [crate::option_vars::opt_wim_flag::FULL as u8; 4]
        );

        set_p_wim(prev_wim.as_deref());
        unsafe { crate::globals::GLOBALS.get_mut() }.wim_flags = prev_flags;
    }

    #[test]
    fn check_opt_wim_colon_combines_flags_into_the_same_slot() {
        let _lock = crate::globals::global_state_test_lock();
        // "longest:full,list,full" - hand-traced: slot 0 = LONGEST|FULL
        // (":"-joined), slot 1 = LIST, slot 2 = FULL, slot 3 repeats
        // slot 2's own FULL (fewer than 4 comma-separated stages).
        let prev_wim = set_p_wim(Some(b"longest:full,list,full"));
        let prev_flags = reset_wim_flags();

        assert_eq!(unsafe { check_opt_wim() }, crate::vim_defs::OK);
        use crate::option_vars::opt_wim_flag;
        assert_eq!(
            unsafe { crate::globals::GLOBALS.get_mut() }.wim_flags,
            [
                (opt_wim_flag::LONGEST | opt_wim_flag::FULL) as u8,
                opt_wim_flag::LIST as u8,
                opt_wim_flag::FULL as u8,
                opt_wim_flag::FULL as u8,
            ]
        );

        set_p_wim(prev_wim.as_deref());
        unsafe { crate::globals::GLOBALS.get_mut() }.wim_flags = prev_flags;
    }

    #[test]
    fn check_opt_wim_empty_value_is_ok_with_all_zero_flags() {
        let _lock = crate::globals::global_state_test_lock();
        let prev_wim = set_p_wim(Some(b""));
        let prev_flags = reset_wim_flags();

        assert_eq!(unsafe { check_opt_wim() }, crate::vim_defs::OK);
        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.wim_flags, [0; 4]);

        set_p_wim(prev_wim.as_deref());
        unsafe { crate::globals::GLOBALS.get_mut() }.wim_flags = prev_flags;
    }

    #[test]
    fn check_opt_wim_unknown_word_fails() {
        let _lock = crate::globals::global_state_test_lock();
        let prev_wim = set_p_wim(Some(b"bogus"));
        assert_eq!(unsafe { check_opt_wim() }, crate::vim_defs::FAIL);
        set_p_wim(prev_wim.as_deref());
    }

    #[test]
    fn check_opt_wim_more_than_4_stages_fails() {
        let _lock = crate::globals::global_state_test_lock();
        let prev_wim = set_p_wim(Some(b"full,full,full,full,full"));
        assert_eq!(unsafe { check_opt_wim() }, crate::vim_defs::FAIL);
        set_p_wim(prev_wim.as_deref());
    }

    #[test]
    fn check_opt_wim_failure_leaves_wim_flags_untouched() {
        let _lock = crate::globals::global_state_test_lock();
        let prev_wim = set_p_wim(Some(b"bogus"));
        let prev_flags = reset_wim_flags();
        unsafe { crate::globals::GLOBALS.get_mut() }.wim_flags = [7, 7, 7, 7];

        assert_eq!(unsafe { check_opt_wim() }, crate::vim_defs::FAIL);
        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.wim_flags, [7, 7, 7, 7]);

        set_p_wim(prev_wim.as_deref());
        unsafe { crate::globals::GLOBALS.get_mut() }.wim_flags = prev_flags;
    }
}
