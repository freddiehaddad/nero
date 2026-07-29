//! Translated from `src/nvim/buffer.c` (tractable core only). This is one
//! of the largest, most cross-cutting files in the whole codebase
//! (~4349 lines, 91 top-level functions): almost every function needs
//! real file I/O (`readfile`/`ml_open`, `os/fs.c`), `ctx_switch`/window
//! management, `autocmd.c`'s `apply_autocmds`, the eval engine's `b:`
//! dict watcher machinery, or `memline.c`'s `ml_get`/`ml_get_buf`.
//!
//! Translated: `calc_percentage`, `get_highest_fnum` (+ its own
//! `top_file_num` private counter, mirrored as `TOP_FILE_NUM` since
//! the original keeps it as a file-static, not an `EXTERN` global -
//! same treatment as `memfile.c`'s own per-file statics), `set_bufref`/
//! `bufref_valid`/`buf_valid` (+ its own `buf_free_count` private
//! counter, `BUF_FREE_COUNT`), `buflist_findnr` (an O(n)
//! `lastbuf`/`b_prev` walk in place of the original's O(1)
//! `buffer_handles` hash-map lookup, `handle_get_buffer` - the same
//! "linked-list walk replaces an untranslated hash map" treatment
//! already established by `window.rs`'s own `win_find_by_handle`),
//! the `'buftype'`-testing predicate
//! family `bt_prompt`/`bt_cmdwin`/`bt_help`/`bt_normal`/`bt_quickfix`/
//! `bt_terminal`/`bt_nofilename`/`bt_nofile`/`bt_dontwrite`/
//! `bt_dontwrite_msg` (the latter's real `emsg()` display omitted,
//! matching the established "skip the deferred-subsystem side effect,
//! keep the state/return value correct" policy), `buf_hide`,
//! `buf_is_empty` (now tractable now that `memline.c`'s `ml_get_buf`
//! exists), `buffer.h`'s `buf_meta_total` (a tiny `static inline`
//! header function, not from `buffer.c` itself - harvested for
//! `drawscreen.c`'s `number_width`), `curbuf_reusable` (via already-
//! existing `buf_is_empty`/`bt_quickfix`/`crate::undo::
//! curbuf_is_changed`), `buflist_name_nr` (via already-existing
//! `buflist_findnr`/`buflist_findlnum` - returns `Option<(Vec<u8>,
//! LinenrT)>` in place of the original's own out-parameters); and now
//! that `eval/typval_defs.rs`'s
//! `TypvalT`/`ChangedtickDictItem` are real (not opaque placeholders),
//! `buf_get_changedtick`/`buf_set_changedtick`/`buf_inc_changedtick`/
//! `buf_init_changedtick` - each skips only the real dict-watcher
//! notification/`b_vars` registration side effect specifically (see
//! `buf_set_changedtick`/`buf_init_changedtick`'s own doc comments for
//! exactly what each still needs - `DictT`'s own `watchers` field and
//! a sound `ChangedtickDictItem`-as-`dictitem_T` lookup mechanism,
//! respectively, neither of which is simply "`dict_T` doesn't exist"
//! anymore), keeping the underlying `b:changedtick` value itself fully
//! correct for every other C-level accessor in this crate. `set_buflisted`
//! (now tractable now that `autocmd.c`'s `apply_autocmds` is real - see
//! `crate::autocmd`'s own module doc).
//!
//! Deferred (each needs a not-yet-translated subsystem):
//! - `bt_nofileread` (`static`): its only caller, `open_buffer`, is
//!   itself deferred (real file I/O) - translating it now would be
//!   genuinely dead code.
//! - `read_buffer`/`buf_ensure_loaded`/`open_buffer`/`close_buffer`/
//!   `buf_freeall`/`free_buffer`/`buflist_new`/`buflist_getfile`/
//!   `buflist_findnr`/etc.: need real file I/O, `ctx_switch` (window
//!   management), and `autocmd.c`.
//! - `buf_contents_changed`/`wipe_buffer`: need `buflist_new`,
//!   `ctx_switch` (the real switch, not just `ctx_restore`'s bypass
//!   path), `block_autocmds`/`unblock_autocmds`, `readfile`, and
//!   `close_buffer` - `apply_autocmds` alone isn't enough to unblock
//!   these two (re-verified directly against the real source, not
//!   assumed from the old blanket note grouping them with
//!   `set_buflisted`).
//! - Everything else in this file (buffer-list management, window-
//!   buffer association, `:buffer`/`:ls`/title-bar formatting, modeline
//!   processing, etc.): each needs 2+ of the above, plus `tag.c`/
//!   `quickfix.c`/`window.c`.

use crate::buffer_defs::{BufT, BufrefT};
use crate::ex_cmds_defs::cmod;
use crate::globals::{GlobalCell, GLOBALS};
use crate::option_vars::OPTION_VARS;

/// Highest-ever-assigned buffer number counter (`buffer.c`'s own
/// file-static `top_file_num`), starting at 1 like the original.
static TOP_FILE_NUM: GlobalCell<i32> = GlobalCell::new(1);

/// Incremented every time a `buf_T` is freed, letting [`bufref_valid`]
/// skip a full buffer-list walk when nothing has been freed since
/// [`set_bufref`] was called (`buffer.c`'s own file-static
/// `buf_free_count`).
static BUF_FREE_COUNT: GlobalCell<i32> = GlobalCell::new(0);

/// Returns byte `idx` of an option modeled as `Option<Vec<u8>>`, or NUL
/// (`0`) if unset/short - matches how the original dereferences
/// `buf->b_p_bt[idx]`/`buf->b_p_bh[idx]` (a `char *` that in practice is
/// always at least NUL-terminated, never truly `NULL`).
fn opt_byte(opt: &Option<Vec<u8>>, idx: usize) -> u8 {
    opt.as_deref().and_then(|s| s.get(idx)).copied().unwrap_or(0)
}

/// Calculate the percentage that `part` is of `whole` (`calc_percentage`).
#[must_use]
pub fn calc_percentage(part: i64, whole: i64) -> i32 {
    // With 32 bit longs and more than 21,474,836 lines multiplying by 100
    // causes an overflow, thus for large numbers divide instead.
    if part > 1_000_000 {
        (part / (whole / 100)) as i32
    } else {
        ((part * 100) / whole) as i32
    }
}

/// The highest possible buffer number (`get_highest_fnum`).
///
/// # Safety
/// Touches a `GlobalCell` - same requirement as every other function
/// that does so: no overlapping live access.
#[must_use]
pub unsafe fn get_highest_fnum() -> i32 {
    unsafe { *TOP_FILE_NUM.get_mut() - 1 }
}

/// Fill in `bufref` to later check with [`bufref_valid`] whether `buf` is
/// still a valid, live buffer (`set_bufref`).
///
/// # Safety
/// Touches a `GlobalCell` - same requirement as every other function
/// that does so: no overlapping live access.
pub unsafe fn set_bufref(bufref: &mut BufrefT, buf: Option<&BufT>) {
    bufref.br_buf = match buf {
        Some(b) => b as *const BufT as *mut BufT,
        None => std::ptr::null_mut(),
    };
    bufref.br_fnum = buf.map_or(0, |b| b.handle);
    bufref.br_buf_free_count = unsafe { *BUF_FREE_COUNT.get_mut() };
}

/// Return true if `bufref->br_buf` points to the same buffer as when
/// [`set_bufref`] was called and it is a valid buffer. Only goes through
/// the buffer list if `buf_free_count` changed. Also checks if `b_fnum`
/// is still the same, since a `:bwipe` followed by `:new` might get the
/// same allocated memory, but it's a different buffer (`bufref_valid`).
///
/// # Safety
/// `bufref.br_buf`, if non-null, must point to a live `BufT` (or one
/// still reachable via the `lastbuf`/`b_prev` chain). Touches a
/// `GlobalCell` - same requirement as every other function that does so:
/// no overlapping live access.
#[must_use]
pub unsafe fn bufref_valid(bufref: &BufrefT) -> bool {
    if bufref.br_buf_free_count == unsafe { *BUF_FREE_COUNT.get_mut() } {
        return true;
    }
    (unsafe { buf_valid(bufref.br_buf) })
        && bufref.br_fnum == unsafe { &*bufref.br_buf }.handle
}

/// Check that `buf` points to a valid buffer in the buffer list. Can be
/// slow if there are many buffers, prefer using [`bufref_valid`]
/// (`buf_valid`).
///
/// # Safety
/// Touches `crate::globals::GLOBALS` - same requirement as every other
/// function that does so: no overlapping live access.
#[must_use]
pub unsafe fn buf_valid(buf: *const BufT) -> bool {
    if buf.is_null() {
        return false;
    }
    // Assume that we more often have a recent buffer, start with the
    // last one.
    let mut bp: *mut BufT = unsafe { GLOBALS.get_mut() }.lastbuf;
    while !bp.is_null() {
        if std::ptr::eq(bp as *const BufT, buf) {
            return true;
        }
        bp = unsafe { &*bp }.b_prev;
    }
    false
}

/// Find a buffer in the buffer list by its number (buffer handle)
/// (`buflist_findnr`). `nr == 0` means the alternate buffer for the
/// current window (`curwin.w_alt_fnum`).
///
/// The original looks this up in O(1) via a `buffer_handles` hash map
/// (`handle_get_buffer`); that map itself isn't translated, so this
/// walks `GLOBALS.lastbuf`/`b_prev` instead - the exact same
/// "linked-list walk replaces an untranslated hash map" translation
/// already established by `window.rs`'s own `win_find_by_handle`
/// (walking `firstwin`/`w_next` in place of a similar handle lookup).
///
/// # Safety
/// Touches `crate::globals::GLOBALS`; its own `lastbuf`/`b_prev` chain
/// and `curwin` must consist of valid, live pointers.
#[must_use]
pub unsafe fn buflist_findnr(nr: i32) -> *mut BufT {
    let nr = if nr == 0 {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &*GLOBALS.get_mut().curwin }.w_alt_fnum
    } else {
        nr
    };

    // SAFETY: forwarded from this function's own safety doc.
    let mut bp: *mut BufT = unsafe { GLOBALS.get_mut() }.lastbuf;
    while !bp.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { &*bp }.handle == nr {
            return bp;
        }
        // SAFETY: forwarded from this function's own safety doc.
        bp = unsafe { &*bp }.b_prev;
    }
    std::ptr::null_mut()
}

/// `true` if `buf` is a prompt buffer (`bt_prompt`).
#[must_use]
pub fn bt_prompt(buf: Option<&BufT>) -> bool {
    buf.is_some_and(|b| opt_byte(&b.b_p_bt, 0) == b'p')
}

/// `true` if `buf` is the `cmdwin` scratch buffer (`bt_cmdwin`).
///
/// # Safety
/// Touches `crate::globals::GLOBALS` - same requirement as every other
/// function that does so: no overlapping live access.
#[must_use]
pub unsafe fn bt_cmdwin(buf: Option<&BufT>) -> bool {
    match buf {
        Some(b) => std::ptr::eq(
            b as *const BufT,
            unsafe { GLOBALS.get_mut() }.cmdwin_buf as *const BufT,
        ),
        None => false,
    }
}

/// `true` if `buf` is a help buffer (`bt_help`).
#[must_use]
pub fn bt_help(buf: Option<&BufT>) -> bool {
    buf.is_some_and(|b| b.b_help)
}

/// `true` if `buf` has `'buftype'` empty, i.e. a normal buffer
/// (`bt_normal`).
#[must_use]
pub fn bt_normal(buf: Option<&BufT>) -> bool {
    buf.is_some_and(|b| opt_byte(&b.b_p_bt, 0) == 0)
}

/// `true` if `buf` is a quickfix buffer (`bt_quickfix`).
#[must_use]
pub fn bt_quickfix(buf: Option<&BufT>) -> bool {
    buf.is_some_and(|b| opt_byte(&b.b_p_bt, 0) == b'q')
}

/// `true` if `buf` is a terminal buffer (`bt_terminal`).
#[must_use]
pub fn bt_terminal(buf: Option<&BufT>) -> bool {
    buf.is_some_and(|b| opt_byte(&b.b_p_bt, 0) == b't')
}

/// `true` if `buf` is "nofile", "acwrite", a terminal, or a prompt
/// buffer - i.e. has no real backing file name (`bt_nofilename`).
#[must_use]
pub fn bt_nofilename(buf: Option<&BufT>) -> bool {
    buf.is_some_and(|b| {
        (opt_byte(&b.b_p_bt, 0) == b'n' && opt_byte(&b.b_p_bt, 2) == b'f')
            || opt_byte(&b.b_p_bt, 0) == b'a'
            || !b.terminal.is_null()
            || opt_byte(&b.b_p_bt, 0) == b'p'
    })
}

/// `true` if `buf` has `'buftype'` set to "nofile" (`bt_nofile`).
#[must_use]
pub fn bt_nofile(buf: Option<&BufT>) -> bool {
    buf.is_some_and(|b| opt_byte(&b.b_p_bt, 0) == b'n' && opt_byte(&b.b_p_bt, 2) == b'f')
}

/// `true` if `buf` is "nowrite", "nofile", terminal, or prompt - i.e.
/// should not be written to its file (`bt_dontwrite`).
#[must_use]
pub fn bt_dontwrite(buf: Option<&BufT>) -> bool {
    buf.is_some_and(|b| {
        opt_byte(&b.b_p_bt, 0) == b'n' || !b.terminal.is_null() || opt_byte(&b.b_p_bt, 0) == b'p'
    })
}

/// Like [`bt_dontwrite`], but also reports (via a real, reachable
/// `emsg("E382: ...")` call in the original) that writing was
/// disallowed (`bt_dontwrite_msg`).
///
/// The message display itself is omitted - `message.c`'s display
/// pipeline is still not tractable - but the return value (the actual
/// observable behavior every real caller checks) is fully correct,
/// matching the established "skip the deferred-subsystem side effect,
/// keep the state/return value correct" policy used throughout this
/// crate (e.g. `u_get_headentry`/`u_getbot`/`mf_write`'s own omitted
/// `iemsg`/`emsg` calls). Has no real translated caller yet (its own
/// callers are all in `ex_cmds.c`, not translated) - translated ahead
/// of one anyway since it's a small, simple, mechanically-correct
/// wrapper with no design freedom to get wrong, matching this crate's
/// precedent for `ops_defs.rs`'s `OpType`/`expand_T`'s struct shape.
#[must_use]
pub fn bt_dontwrite_msg(buf: Option<&BufT>) -> bool {
    bt_dontwrite(buf)
}

/// `true` if the buffer should be hidden, according to `'bufhidden'`,
/// `'hidden'`, and `":hide"` (`buf_hide`).
///
/// # Safety
/// Touches `crate::globals::GLOBALS` and `crate::option_vars::OPTION_VARS`,
/// each with the same requirement as every other function that touches
/// them: no overlapping live access.
#[must_use]
pub unsafe fn buf_hide(buf: &BufT) -> bool {
    // 'bufhidden' overrules 'hidden' and ":hide", check it first.
    match opt_byte(&buf.b_p_bh, 0) {
        b'u' | b'w' | b'd' => return false, // "unload"/"wipe"/"delete"
        b'h' => return true,                // "hide"
        _ => {}
    }
    unsafe { OPTION_VARS.get_mut() }.p_hid != 0
        || unsafe { GLOBALS.get_mut() }.cmdmod.cmod_flags & cmod::HIDE != 0
}

/// Return true if `buf` is empty: exactly one line, and that line is
/// itself empty (`buf_is_empty`).
///
/// # Safety
/// `buf.b_ml.ml_mfp`, if non-null, must be a valid pointer to a live
/// `MemfileT` (same requirement as `crate::memline::ml_get_buf`).
#[must_use]
pub unsafe fn buf_is_empty(buf: &mut BufT) -> bool {
    buf.b_ml.ml_line_count == 1 && unsafe { crate::memline::ml_get_buf(buf, 1) }[0] == 0
}

/// Return `true` if the current buffer's memory can be re-used, e.g.
/// for `":enew"` (`curbuf_reusable`). Reads `crate::globals::GLOBALS
/// .curbuf` directly (matching the original's own unconditional
/// `curbuf` reliance).
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf`, if non-null, must be a valid
/// pointer to a live `BufT`, and its `b_ml.ml_mfp` (if non-null) must
/// be a valid pointer to a live `MemfileT` (forwarded to
/// [`buf_is_empty`]'s own requirement).
#[must_use]
pub unsafe fn curbuf_reusable() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
    if curbuf.is_null() {
        return false;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let buf = unsafe { &mut *curbuf };
    buf.b_ffname.is_none()
        && buf.b_nwindows <= 1
        && buf.terminal.is_null()
        && (buf.b_ml.ml_mfp.is_null() || unsafe { buf_is_empty(buf) })
        && !bt_quickfix(Some(buf))
        && !unsafe { crate::undo::curbuf_is_changed() }
}

/// Return the total count of a given kind of extmark metadata in
/// `buf` (`buf_meta_total`). Actually a `static inline` function in
/// `buffer.h`, not `buffer.c` itself - harvested here alongside its
/// real caller, `drawscreen.c`'s `number_width`, since `buffer.h` has
/// no dedicated module of its own in this crate.
#[must_use]
pub fn buf_meta_total(buf: &BufT, m: crate::marktree_defs::MetaIndex) -> u32 {
    buf.b_marktree.meta_root[m as usize]
}

/// Whether `buf` has any signs placed (`buf_has_signs`, `sign.c`).
///
/// Nothing translated in this crate can currently place a sign (no
/// `:sign place` command, no extmark-with-sign-properties support),
/// so [`buf_meta_total`] for both sign-related [`crate::marktree_defs::MetaIndex`]
/// variants is always 0 today - this is the real check (not a
/// hardcoded stub), matching the established "always-real-fast-path"
/// pattern (e.g. `has_any_folding`), correct and complete as written
/// for every buffer this crate can currently construct.
#[must_use]
pub fn buf_has_signs(buf: &BufT) -> bool {
    buf_meta_total(buf, crate::marktree_defs::MetaIndex::SignHl) != 0
        || buf_meta_total(buf, crate::marktree_defs::MetaIndex::SignText) != 0
}

/// Check that `wip` has `'diff'` set and the diff is only for another
/// tab page - a diff is local to a tab page (`wininfo_other_tab_diff`).
///
/// The original's own `FOR_ALL_WINDOWS_IN_TAB(wp, curtab)` always
/// resolves to `GLOBALS.firstwin` here (comparing `curtab` to itself),
/// matching the same simplification already established elsewhere in
/// this crate (e.g. `mark.rs`'s `fmarks_check_names`).
///
/// # Safety
/// `wip.wi_win`, if non-null, must be a valid pointer to a live
/// `WinT`. Touches `GLOBALS.firstwin`/`w_next` - same requirement as
/// every other function that walks the window list.
#[must_use]
pub unsafe fn wininfo_other_tab_diff(wip: &crate::buffer_defs::WinInfo) -> bool {
    if wip.wi_opt.wo_diff == 0 {
        return false;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let mut wp = unsafe { GLOBALS.get_mut() }.firstwin;
    while !wp.is_null() {
        if std::ptr::eq(wip.wi_win, wp) {
            // It's a window in the current tab page, so the buffer
            // was in diff mode here.
            return false;
        }
        // SAFETY: forwarded from this function's own safety doc.
        wp = unsafe { &*wp }.w_next;
    }
    true
}

/// Find info for the current window in buffer `buf`. If not found,
/// return the info for the most recently used window
/// (`find_wininfo`).
///
/// `need_options`: when true, skip entries where `wi_optset` is
/// false. `skip_diff_buffer`: when true, avoid windows with
/// `'diff'` set that is in another tab page.
///
/// # Safety
/// Every entry in `buf.b_wininfo` must be a valid, non-null pointer to
/// a live `WinInfo`. Touches `GLOBALS.curwin`/`firstwin` - same
/// requirement as every other function that does so.
#[must_use]
pub unsafe fn find_wininfo(
    buf: &BufT,
    need_options: bool,
    skip_diff_buffer: bool,
) -> *mut crate::buffer_defs::WinInfo {
    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { GLOBALS.get_mut() }.curwin;
    for &wip_ptr in &buf.b_wininfo {
        // SAFETY: forwarded from this function's own safety doc.
        let wip = unsafe { &*wip_ptr };
        if std::ptr::eq(wip.wi_win, curwin)
            // SAFETY: forwarded from this function's own safety doc.
            && (!skip_diff_buffer || !unsafe { wininfo_other_tab_diff(wip) })
            && (!need_options || wip.wi_optset)
        {
            return wip_ptr;
        }
    }

    // If no wininfo for curwin, use the first in the list (that
    // doesn't have 'diff' set and is in another tab page). If
    // "need_options" is true skip entries that don't have options
    // set, unless the window is editing "buf", so we can copy from
    // the window itself.
    if skip_diff_buffer {
        for &wip_ptr in &buf.b_wininfo {
            // SAFETY: forwarded from this function's own safety doc.
            let wip = unsafe { &*wip_ptr };
            // SAFETY: forwarded from this function's own safety doc.
            if !unsafe { wininfo_other_tab_diff(wip) }
                && (!need_options
                    || wip.wi_optset
                    || (!wip.wi_win.is_null()
                        // SAFETY: forwarded from this function's own safety doc.
                        && std::ptr::eq(unsafe { &*wip.wi_win }.w_buffer, buf as *const BufT as *mut BufT)))
            {
                return wip_ptr;
            }
        }
    } else if let Some(&first) = buf.b_wininfo.first() {
        return first;
    }
    std::ptr::null_mut()
}

/// The original's own function-local `static fmark_T no_position`
/// fallback in [`buflist_findfmark`] - line 1, matching a sensible
/// "no recorded position" default (deliberately NOT `FmarkT::default()`,
/// which would report line 0, an invalid position).
fn no_position_fmark() -> crate::mark_defs::FmarkT {
    crate::mark_defs::FmarkT {
        mark: crate::pos_defs::PosT { lnum: 1, col: 0, coladd: 0 },
        ..crate::mark_defs::FmarkT::default()
    }
}

/// Get the file mark for `buf` (`buflist_findfmark`).
///
/// Returns an owned [`crate::mark_defs::FmarkT`] rather than the
/// original's `fmark_T *` (a pointer either into `buf`'s own
/// `b_wininfo` entries or to a function-local `static` fallback) -
/// nothing currently in this crate needs to mutate through the
/// returned value, and a `'static`-lifetime pointer to a genuinely
/// per-call fallback value has no sound direct Rust equivalent
/// anyway.
///
/// # Safety
/// Forwarded from [`find_wininfo`]'s own safety doc.
#[must_use]
pub unsafe fn buflist_findfmark(buf: &BufT) -> crate::mark_defs::FmarkT {
    // SAFETY: forwarded from this function's own safety doc.
    let wip = unsafe { find_wininfo(buf, false, false) };
    if wip.is_null() {
        return no_position_fmark();
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { &*wip }.wi_mark.clone()
}

/// Find the line number for buffer `buf` for the current window
/// (`buflist_findlnum`).
///
/// # Safety
/// Forwarded from [`buflist_findfmark`]'s own safety doc.
#[must_use]
pub unsafe fn buflist_findlnum(buf: &BufT) -> crate::pos_defs::LinenrT {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { buflist_findfmark(buf) }.mark.lnum
}

/// Look up buffer `fnum`'s own short file name and remembered line
/// number (`buflist_name_nr`). Returns `Some((fname, lnum))` in place
/// of the original's own `char **fname`/`linenr_T *lnum` out-
/// parameters plus a separate `OK`/`FAIL` result - `None` exactly
/// where the original returns `FAIL` (no such buffer, or the buffer
/// has no short file name).
///
/// # Safety
/// Forwarded from [`buflist_findnr`]/[`buflist_findlnum`]'s own
/// safety docs.
#[must_use]
pub unsafe fn buflist_name_nr(fnum: i32) -> Option<(Vec<u8>, crate::pos_defs::LinenrT)> {
    // SAFETY: forwarded from this function's own safety doc.
    let buf = unsafe { buflist_findnr(fnum) };
    if buf.is_null() {
        return None;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let buf = unsafe { &*buf };
    let fname = buf.b_fname.clone()?;
    // SAFETY: forwarded from this function's own safety doc.
    let lnum = unsafe { buflist_findlnum(buf) };
    Some((fname, lnum))
}

/// Get `b:changedtick` value. Faster than querying `b:`
/// (`buf_get_changedtick`, `buffer.h`'s own `static inline`).
#[must_use]
pub fn buf_get_changedtick(buf: &BufT) -> crate::eval::typval_defs::VarnumberT {
    match buf.changedtick_di.di_tv.value {
        crate::eval::typval_defs::TypvalValue::Number(n) => n,
        // Not yet initialized via buf_init_changedtick - matches
        // reading an all-zero union in the original, which would also
        // read 0.
        _ => 0,
    }
}

/// Set `b:changedtick`, also checking `b:` for consistency in debug
/// builds in the original (`buf_set_changedtick`).
///
/// # Deferred
/// The original also notifies any dict watchers on `buf->b_vars` of
/// the change (`tv_dict_watcher_notify`) - not done here. `dict_T`
/// itself is real now (as [`crate::eval::typval_defs::DictT`]), but
/// its `watchers` field (a `QUEUE` of dict-key watchers set by user
/// code, e.g. `dictwatcheradd()`) is still deferred - needs a `QUEUE`
/// intrusive-linked-list translation first, see `DictT`'s own doc
/// comment. `b_vars` itself is also always null in this crate so far
/// (nothing allocates a real per-buffer dict yet - see
/// [`buf_init_changedtick`]'s own doc comment for the further
/// complication even once it is). The underlying value itself is
/// still set correctly, and every other C-level accessor in this
/// crate reads it directly (not through the dict), so this gap only
/// affects Vimscript-visible `b:changedtick` watchers, not this
/// crate's own internal bookkeeping.
pub fn buf_set_changedtick(buf: &mut BufT, changedtick: crate::eval::typval_defs::VarnumberT) {
    buf.changedtick_di.di_tv.value = crate::eval::typval_defs::TypvalValue::Number(changedtick);
}

/// Increment `b:changedtick` value. Also checks `b:` for consistency
/// in debug builds in the original (`buf_inc_changedtick`).
pub fn buf_inc_changedtick(buf: &mut BufT) {
    buf_set_changedtick(buf, buf_get_changedtick(buf) + 1);
}

/// Initialize `buf.changedtick_di` (`buf_init_changedtick`,
/// `static inline` in the original).
///
/// # Deferred
/// The original also registers this item into `buf->b_vars` (via
/// `tv_dict_add`) so Vimscript code can read `b:changedtick` through
/// the dict lookup path - not done here. This needs more than just
/// `dict_T` (real now, and `b_vars` is `*mut DictT` already - see
/// `buffer_defs.rs`): the original casts `&buf->changedtick_di` (a
/// `ChangedtickDictItem`, the fixed-key-size `TV_DICTITEM_STRUCT`
/// instantiation) to a plain `dictitem_T *`, relying on their
/// byte-identical C struct layout - a cast this crate's separate,
/// unrelated `ChangedtickDictItem`/`DictitemT` Rust types cannot
/// soundly replicate (and [`crate::eval::typval_defs::DictT`]'s own
/// `dv_index` side table is typed `*mut DictitemT` specifically, not
/// an untyped pointer, so it has nowhere to put a
/// `*mut ChangedtickDictItem` even if the cast were sound). A real
/// fix needs its own design pass (e.g. a shared trait, an untyped
/// `dv_index` value, or a different lookup mechanism entirely) -
/// deliberately not attempted here. `b_vars` is also always null in
/// this crate so far, so there is nothing to insert into yet either.
/// [`buf_get_changedtick`]/[`buf_set_changedtick`] (this crate's own
/// C-level accessors) already read/write the real value directly,
/// independent of this dict registration.
pub fn buf_init_changedtick(buf: &mut BufT) {
    buf.changedtick_di = crate::eval::typval_defs::ChangedtickDictItem {
        di_flags: crate::eval::typval_defs::dict_item_flags::RO
            | crate::eval::typval_defs::dict_item_flags::FIX,
        di_tv: crate::eval::typval_defs::TypvalT {
            v_lock: crate::eval::typval_defs::VarLockStatus::Fixed,
            value: crate::eval::typval_defs::TypvalValue::Number(buf_get_changedtick(buf)),
        },
        di_key: b"changedtick".to_vec(),
    };
}

/// Set `'buflisted'` for `curbuf` to `on` and trigger autocommands if
/// it changed (`set_buflisted`).
///
/// Now tractable now that `autocmd.c`'s `apply_autocmds` is real (see
/// `crate::autocmd`'s own module doc) - currently always a real no-op
/// beyond the `b_p_bl` flip itself, since `AUTOCMDS` is always empty
/// today.
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer
/// to a live `BufT`.
pub unsafe fn set_buflisted(on: bool) {
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &mut *crate::globals::GLOBALS.get_mut().curbuf };
    let on = i32::from(on);
    if on == curbuf.b_p_bl {
        return;
    }

    curbuf.b_p_bl = on;
    let event = if on != 0 {
        crate::autocmd_defs::EventT::BufAdd
    } else {
        crate::autocmd_defs::EventT::BufDelete
    };
    let _ = crate::autocmd::apply_autocmds(event, None, None, false, Some(&*curbuf));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer_defs::BufT;

    #[test]
    fn buf_get_changedtick_defaults_to_zero_before_init() {
        let buf = BufT::default();
        assert_eq!(buf_get_changedtick(&buf), 0);
    }

    #[test]
    fn buf_set_and_get_changedtick_roundtrip() {
        let mut buf = BufT::default();
        buf_set_changedtick(&mut buf, 5);
        assert_eq!(buf_get_changedtick(&buf), 5);
    }

    #[test]
    fn buf_inc_changedtick_increments_by_one() {
        let mut buf = BufT::default();
        buf_set_changedtick(&mut buf, 5);
        buf_inc_changedtick(&mut buf);
        assert_eq!(buf_get_changedtick(&buf), 6);
    }

    #[test]
    fn buf_inc_changedtick_from_default_starts_at_one() {
        let mut buf = BufT::default();
        buf_inc_changedtick(&mut buf);
        assert_eq!(buf_get_changedtick(&buf), 1);
    }

    #[test]
    fn buf_init_changedtick_sets_flags_lock_and_key() {
        let mut buf = BufT::default();
        buf_init_changedtick(&mut buf);

        assert_eq!(
            buf.changedtick_di.di_flags,
            crate::eval::typval_defs::dict_item_flags::RO
                | crate::eval::typval_defs::dict_item_flags::FIX
        );
        assert_eq!(buf.changedtick_di.di_tv.v_lock, crate::eval::typval_defs::VarLockStatus::Fixed);
        assert_eq!(buf.changedtick_di.di_key, b"changedtick");
        // Starts at whatever buf_get_changedtick already reported (0
        // for a fresh, never-set buffer).
        assert_eq!(buf_get_changedtick(&buf), 0);
    }

    #[test]
    fn buf_init_changedtick_preserves_a_prior_value() {
        let mut buf = BufT::default();
        buf_set_changedtick(&mut buf, 42);
        buf_init_changedtick(&mut buf);
        assert_eq!(buf_get_changedtick(&buf), 42);
    }

    fn buf_with_bt(bt: &str) -> BufT {
        BufT {
            b_p_bt: Some(bt.as_bytes().to_vec()),
            ..Default::default()
        }
    }

    #[test]
    fn calc_percentage_matches_c_arithmetic() {
        assert_eq!(calc_percentage(50, 100), 50);
        assert_eq!(calc_percentage(1, 3), 33);
        assert_eq!(calc_percentage(2_000_000, 4_000_000), 50);
    }

    #[test]
    fn bt_prompt_checks_first_byte() {
        assert!(bt_prompt(Some(&buf_with_bt("prompt"))));
        assert!(!bt_prompt(Some(&buf_with_bt("nofile"))));
        assert!(!bt_prompt(None));
    }

    #[test]
    fn bt_normal_is_true_only_for_empty_buftype() {
        assert!(bt_normal(Some(&buf_with_bt(""))));
        assert!(!bt_normal(Some(&buf_with_bt("help"))));
    }

    #[test]
    fn bt_quickfix_and_terminal_check_first_byte() {
        assert!(bt_quickfix(Some(&buf_with_bt("quickfix"))));
        assert!(bt_terminal(Some(&buf_with_bt("terminal"))));
        assert!(!bt_quickfix(Some(&buf_with_bt("terminal"))));
    }

    #[test]
    fn bt_nofile_checks_no_and_f_bytes() {
        assert!(bt_nofile(Some(&buf_with_bt("nofile"))));
        assert!(!bt_nofile(Some(&buf_with_bt("nowrite"))));
    }

    #[test]
    fn bt_nofilename_covers_nofile_acwrite_terminal_prompt() {
        assert!(bt_nofilename(Some(&buf_with_bt("nofile"))));
        assert!(bt_nofilename(Some(&buf_with_bt("acwrite"))));
        assert!(bt_nofilename(Some(&buf_with_bt("prompt"))));
        assert!(!bt_nofilename(Some(&buf_with_bt("help"))));
    }

    #[test]
    fn bt_dontwrite_covers_nowrite_terminal_prompt() {
        assert!(bt_dontwrite(Some(&buf_with_bt("nowrite"))));
        assert!(bt_dontwrite(Some(&buf_with_bt("prompt"))));
        assert!(!bt_dontwrite(Some(&buf_with_bt("help"))));
    }

    #[test]
    fn bt_dontwrite_msg_matches_bt_dontwrite_return_value() {
        // The message display itself is omitted (message.c not
        // tractable), but the return value must exactly match
        // bt_dontwrite's own - true for every case it covers, and
        // false (with no message) for a buffer it doesn't cover.
        assert!(bt_dontwrite_msg(Some(&buf_with_bt("nowrite"))));
        assert!(bt_dontwrite_msg(Some(&buf_with_bt("prompt"))));
        assert!(!bt_dontwrite_msg(Some(&buf_with_bt("help"))));
        assert!(!bt_dontwrite_msg(None));
    }

    #[test]
    fn bt_help_checks_b_help_flag() {
        let mut b = BufT::default();
        assert!(!bt_help(Some(&b)));
        b.b_help = true;
        assert!(bt_help(Some(&b)));
    }

    #[test]
    fn buf_valid_returns_false_for_null() {
        // SAFETY: no overlapping GLOBALS access from other threads in tests.
        unsafe {
            assert!(!buf_valid(std::ptr::null()));
        }
    }

    #[test]
    fn set_bufref_and_bufref_valid_roundtrip() {
        let buf = buf_with_bt("");
        let mut bufref = BufrefT::default();
        // SAFETY: single-threaded test, no overlapping GLOBALS access.
        unsafe {
            set_bufref(&mut bufref, Some(&buf));
            assert_eq!(bufref.br_fnum, buf.handle);
            // buf_free_count hasn't changed since set_bufref, so
            // bufref_valid takes the fast path (true) without needing
            // `buf` to actually be linked into the real buffer list.
            assert!(bufref_valid(&bufref));
        }
    }

    #[test]
    fn set_bufref_none_gives_null_buf_and_zero_fnum() {
        let mut bufref = BufrefT::default();
        // SAFETY: single-threaded test, no overlapping GLOBALS access.
        unsafe {
            set_bufref(&mut bufref, None);
        }
        assert!(bufref.br_buf.is_null());
        assert_eq!(bufref.br_fnum, 0);
    }

    #[test]
    fn buf_hide_bufhidden_overrules_hidden_and_cmdmod() {
        let mut b = buf_with_bt("");
        b.b_p_bh = Some(b"hide".to_vec());
        // SAFETY: single-threaded test, no overlapping GLOBALS/OPTION_VARS access.
        unsafe {
            assert!(buf_hide(&b));
            b.b_p_bh = Some(b"unload".to_vec());
            assert!(!buf_hide(&b));
        }
    }

    #[test]
    fn buf_is_empty_true_for_freshly_opened_buffer() {
        // ml_open touches shared GLOBALS.got_int internally via
        // mf_sync - must hold the lock like every other GlobalCell-
        // touching test.
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);

        assert!(unsafe { buf_is_empty(&mut buf) });

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn buf_is_empty_false_when_line_has_content() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(&mut buf, 1, b"x\0") },
            crate::vim_defs::OK
        );

        assert!(!unsafe { buf_is_empty(&mut buf) });

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn buf_is_empty_false_when_more_than_one_line() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        assert_eq!(
            unsafe { crate::memline::ml_append_buf(&mut buf, 1, b"\0", 1, false) },
            crate::vim_defs::OK
        );

        // Two empty lines: still not "empty" per buf_is_empty's own
        // definition (exactly one line).
        assert!(!unsafe { buf_is_empty(&mut buf) });

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    /// Points `GLOBALS.curbuf` at `buf` for the guard's lifetime,
    /// restoring the previous value on drop. Callers must hold
    /// `global_state_test_lock()` for the guard's whole lifetime
    /// (matching `change.rs`'s own identically-named helper).
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
    fn curbuf_reusable_false_when_curbuf_is_null() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = CurbufGuard::set(std::ptr::null_mut());
        assert!(!unsafe { curbuf_reusable() });
    }

    #[test]
    fn curbuf_reusable_true_for_a_fresh_unnamed_empty_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        let _guard = CurbufGuard::set(&mut buf as *mut BufT);

        assert!(unsafe { curbuf_reusable() });

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn curbuf_reusable_false_when_buffer_has_a_filename() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        buf.b_ffname = Some(b"/tmp/example.txt".to_vec());
        let _guard = CurbufGuard::set(&mut buf as *mut BufT);

        assert!(!unsafe { curbuf_reusable() });

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn curbuf_reusable_false_when_more_than_one_window_shows_it() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        buf.b_nwindows = 2;
        let _guard = CurbufGuard::set(&mut buf as *mut BufT);

        assert!(!unsafe { curbuf_reusable() });

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn set_buflisted_is_a_noop_when_value_unchanged() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { b_p_bl: 1, ..Default::default() };
        let _guard = CurbufGuard::set(&mut buf as *mut BufT);

        unsafe { set_buflisted(true) };

        assert_eq!(buf.b_p_bl, 1);
    }

    #[test]
    fn set_buflisted_flips_off_to_on() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { b_p_bl: 0, ..Default::default() };
        let _guard = CurbufGuard::set(&mut buf as *mut BufT);

        unsafe { set_buflisted(true) };

        assert_eq!(buf.b_p_bl, 1);
    }

    #[test]
    fn set_buflisted_flips_on_to_off() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { b_p_bl: 1, ..Default::default() };
        let _guard = CurbufGuard::set(&mut buf as *mut BufT);

        unsafe { set_buflisted(false) };

        assert_eq!(buf.b_p_bl, 0);
    }

    // --- wininfo_other_tab_diff / find_wininfo / buflist_findfmark / buflist_findlnum ---

    #[test]
    fn wininfo_other_tab_diff_false_when_wo_diff_unset() {
        let _lock = crate::globals::global_state_test_lock();
        let win = crate::buffer_defs::WinInfo::default(); // wi_opt.wo_diff == 0
        assert!(!unsafe { wininfo_other_tab_diff(&win) });
    }

    #[test]
    fn wininfo_other_tab_diff_false_when_window_is_in_current_tab() {
        let _lock = crate::globals::global_state_test_lock();
        let mut w = crate::buffer_defs::WinT::default();
        let w_ptr = &mut w as *mut crate::buffer_defs::WinT;
        let prev_firstwin = unsafe { crate::globals::GLOBALS.get_mut() }.firstwin;
        unsafe { crate::globals::GLOBALS.get_mut() }.firstwin = w_ptr;

        let wi = crate::buffer_defs::WinInfo {
            wi_win: w_ptr,
            wi_opt: crate::buffer_defs::WinoptT { wo_diff: 1, ..Default::default() },
            ..Default::default()
        };
        assert!(!unsafe { wininfo_other_tab_diff(&wi) });

        unsafe { crate::globals::GLOBALS.get_mut() }.firstwin = prev_firstwin;
    }

    #[test]
    fn wininfo_other_tab_diff_true_when_diff_set_and_window_not_in_current_tab() {
        let _lock = crate::globals::global_state_test_lock();
        let mut current_win = crate::buffer_defs::WinT::default();
        let prev_firstwin = unsafe { crate::globals::GLOBALS.get_mut() }.firstwin;
        unsafe { crate::globals::GLOBALS.get_mut() }.firstwin = &mut current_win as *mut crate::buffer_defs::WinT;

        // A separate window (never linked into GLOBALS.firstwin's own
        // w_next chain) with 'diff' set - the diff must be for another
        // tab page.
        let mut other_win = crate::buffer_defs::WinT::default();
        let wi = crate::buffer_defs::WinInfo {
            wi_win: &mut other_win as *mut crate::buffer_defs::WinT,
            wi_opt: crate::buffer_defs::WinoptT { wo_diff: 1, ..Default::default() },
            ..Default::default()
        };
        assert!(unsafe { wininfo_other_tab_diff(&wi) });

        unsafe { crate::globals::GLOBALS.get_mut() }.firstwin = prev_firstwin;
    }

    #[test]
    fn find_wininfo_returns_curwin_entry_when_present() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win1 = crate::buffer_defs::WinT::default();
        let mut win2 = crate::buffer_defs::WinT::default();
        let win2_ptr = &mut win2 as *mut crate::buffer_defs::WinT;
        let _guard = CurwinGuard::set(win2_ptr);

        let wi1 = Box::into_raw(Box::new(crate::buffer_defs::WinInfo {
            wi_win: &mut win1 as *mut crate::buffer_defs::WinT,
            ..Default::default()
        }));
        let wi2 = Box::into_raw(Box::new(crate::buffer_defs::WinInfo { wi_win: win2_ptr, ..Default::default() }));
        let buf = BufT { b_wininfo: vec![wi1, wi2], ..Default::default() };

        let found = unsafe { find_wininfo(&buf, false, false) };
        assert_eq!(found, wi2);

        unsafe {
            drop(Box::from_raw(wi1));
            drop(Box::from_raw(wi2));
        }
    }

    #[test]
    fn find_wininfo_falls_back_to_first_entry_when_curwin_not_found() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win1 = crate::buffer_defs::WinT::default();
        let mut unrelated_curwin = crate::buffer_defs::WinT::default();
        let _guard = CurwinGuard::set(&mut unrelated_curwin as *mut crate::buffer_defs::WinT);

        let wi1 = Box::into_raw(Box::new(crate::buffer_defs::WinInfo {
            wi_win: &mut win1 as *mut crate::buffer_defs::WinT,
            ..Default::default()
        }));
        let buf = BufT { b_wininfo: vec![wi1], ..Default::default() };

        // skip_diff_buffer == false -> falls back to the first entry.
        let found = unsafe { find_wininfo(&buf, false, false) };
        assert_eq!(found, wi1);

        unsafe {
            drop(Box::from_raw(wi1));
        }
    }

    #[test]
    fn find_wininfo_returns_null_when_b_wininfo_is_empty() {
        let _lock = crate::globals::global_state_test_lock();
        let mut curwin = crate::buffer_defs::WinT::default();
        let _guard = CurwinGuard::set(&mut curwin as *mut crate::buffer_defs::WinT);

        let buf = BufT::default();
        assert!(unsafe { find_wininfo(&buf, false, false) }.is_null());
    }

    #[test]
    fn buflist_findfmark_returns_no_position_when_not_found() {
        let _lock = crate::globals::global_state_test_lock();
        let mut curwin = crate::buffer_defs::WinT::default();
        let _guard = CurwinGuard::set(&mut curwin as *mut crate::buffer_defs::WinT);

        let buf = BufT::default();
        let fm = unsafe { buflist_findfmark(&buf) };
        // The original's own `no_position` static: line 1, not 0.
        assert_eq!(fm.mark.lnum, 1);
    }

    #[test]
    fn buflist_findlnum_returns_the_wininfo_mark_lnum() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = crate::buffer_defs::WinT::default();
        let win_ptr = &mut win as *mut crate::buffer_defs::WinT;
        let _guard = CurwinGuard::set(win_ptr);

        let wi = Box::into_raw(Box::new(crate::buffer_defs::WinInfo {
            wi_win: win_ptr,
            wi_mark: crate::mark_defs::FmarkT {
                mark: crate::pos_defs::PosT { lnum: 42, col: 0, coladd: 0 },
                ..Default::default()
            },
            ..Default::default()
        }));
        let buf = BufT { b_wininfo: vec![wi], ..Default::default() };

        assert_eq!(unsafe { buflist_findlnum(&buf) }, 42);

        unsafe {
            drop(Box::from_raw(wi));
        }
    }

    /// Points `GLOBALS.curwin` at `win` for the guard's lifetime,
    /// restoring the previous value on drop. Callers must hold
    /// `global_state_test_lock()` for the guard's whole lifetime
    /// (matching this file's own `CurbufGuard`).
    struct CurwinGuard {
        previous: *mut crate::buffer_defs::WinT,
    }

    impl CurwinGuard {
        fn set(new_curwin: *mut crate::buffer_defs::WinT) -> Self {
            let previous = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
            unsafe { crate::globals::GLOBALS.get_mut() }.curwin = new_curwin;
            CurwinGuard { previous }
        }
    }

    impl Drop for CurwinGuard {
        fn drop(&mut self) {
            unsafe { crate::globals::GLOBALS.get_mut() }.curwin = self.previous;
        }
    }

    /// Points `GLOBALS.lastbuf` at `buf` for the guard's lifetime,
    /// restoring the previous value on drop. Callers must hold
    /// `global_state_test_lock()` for the guard's whole lifetime
    /// (matching this file's own `CurbufGuard`/`CurwinGuard`).
    struct LastbufGuard {
        previous: *mut BufT,
    }

    impl LastbufGuard {
        fn set(new_lastbuf: *mut BufT) -> Self {
            let previous = unsafe { crate::globals::GLOBALS.get_mut() }.lastbuf;
            unsafe { crate::globals::GLOBALS.get_mut() }.lastbuf = new_lastbuf;
            LastbufGuard { previous }
        }
    }

    impl Drop for LastbufGuard {
        fn drop(&mut self) {
            unsafe { crate::globals::GLOBALS.get_mut() }.lastbuf = self.previous;
        }
    }

    #[test]
    fn buflist_name_nr_returns_none_for_an_unknown_buffer_number() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = LastbufGuard::set(std::ptr::null_mut());
        assert!(unsafe { buflist_name_nr(42) }.is_none());
    }

    #[test]
    fn buflist_name_nr_returns_none_when_buffer_has_no_short_name() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { handle: 7, b_fname: None, ..Default::default() };
        let _guard = LastbufGuard::set(&mut buf as *mut BufT);
        assert!(unsafe { buflist_name_nr(7) }.is_none());
    }

    #[test]
    fn buflist_name_nr_returns_the_short_name_and_lnum() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = crate::buffer_defs::WinT::default();
        let win_ptr = &mut win as *mut crate::buffer_defs::WinT;
        let _curwin_guard = CurwinGuard::set(win_ptr);

        let wi = Box::into_raw(Box::new(crate::buffer_defs::WinInfo {
            wi_win: win_ptr,
            wi_mark: crate::mark_defs::FmarkT {
                mark: crate::pos_defs::PosT { lnum: 17, col: 0, coladd: 0 },
                ..Default::default()
            },
            ..Default::default()
        }));
        let mut buf = BufT {
            handle: 3,
            b_fname: Some(b"foo.txt".to_vec()),
            b_wininfo: vec![wi],
            ..Default::default()
        };
        let _lastbuf_guard = LastbufGuard::set(&mut buf as *mut BufT);

        let (fname, lnum) = unsafe { buflist_name_nr(3) }.expect("buffer 3 should be found");
        assert_eq!(fname, b"foo.txt");
        assert_eq!(lnum, 17);

        unsafe {
            drop(Box::from_raw(wi));
        }
    }
}
