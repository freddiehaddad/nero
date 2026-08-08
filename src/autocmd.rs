//! Translated from `src/nvim/autocmd.c` (tractable core only).
//!
//! `autocmd.c` (~2500 lines) is the full `:autocmd`/autocommand
//! subsystem: defining/deleting autocmds and augroups, matching
//! patterns against file names, and actually EXECUTING an event's
//! matching autocmds (`apply_autocmds_group`'s real body) - none of
//! the `:autocmd`-definition side is translated here (needs the
//! `:autocmd` command parser and pattern-matching machinery, a
//! separate, substantial undertaking).
//!
//! **What IS translated, and why it's faithful, not a shortcut**:
//! nothing in this crate can currently call a (not yet translated)
//! autocmd-DEFINING function (`au_add_autocmd`/`do_autocmd`), so
//! `AUTOCMDS` - every event's own vector of registered autocmds -
//! is, and can only currently be, always empty. The original's own
//! `apply_autocmds_group` has a "quickly return if there are no
//! autocommands for this event" early check
//! (`kv_size(autocmds[event]) == 0`) specifically for this exact
//! condition - so translating that REAL check (not hardcoding "always
//! return false") makes `apply_autocmds`/`apply_autocmds_exarg`/
//! `apply_autocmds_retval`/`has_event` correct and complete AS
//! WRITTEN, for real, today - and means this code will start
//! correctly executing the (still-`unimplemented!()`) real body the
//! moment a future session translates autocmd-defining machinery,
//! with no revision needed here.
//!
//! Followed the bypass path all the way through to confirm it's
//! genuinely side-effect-complete, not just "returns early without
//! doing anything interesting": `is_autocmd_blocked()` is never
//! reached at all (short-circuited by the empty-vector check, which
//! is always true, in `event == NUM_EVENTS || ... == 0 ||
//! is_autocmd_blocked()`); the `BYPASS_AU:` tail's
//! [`aubuflocal_remove`] (called for `EventT::BufWipeout`) is itself
//! real and faithfully translated, but is ALSO always a no-op today
//! for the same reason (its own two loops walk `ACTIVE_APC_LIST`,
//! always null, and `AUTOCMDS[event]`, always empty) down to its own
//! `au_cleanup` tail call (`au_need_clean` starts `false` and is
//! only ever set by code inside those same always-empty loops, so
//! `au_cleanup`'s own early-return is also always real, always taken);
//! the `retval == OK && event == EVENT_FILETYPE` branch is
//! unreachable within the scope translated here specifically because
//! `retval` is provably `false` throughout (never set `true` before
//! reaching `BYPASS_AU`, since that only happens inside the
//! not-yet-translated real body) - omitted with this comment rather
//! than an `unimplemented!()` guard, since there is no runtime
//! condition under which it could fire yet; and `crate::context`'s
//! `ctx_restore` is a real, already-verified no-op for a
//! never-`ctx_switch`-ed `CtxSwitch` (see its own module doc).
//!
//! `apply_autocmds_retval` additionally needed `ex_eval.c`'s
//! `should_abort`/`aborting` (harvested here since this was their
//! first real caller) - both fully tractable already, needing only
//! already-existing `GLOBALS` fields (`did_emsg`/`force_abort`/
//! `got_int`/`did_throw`/`trylevel`/`emsg_silent`).
//!
//! Also translated: `block_autocmds`/`unblock_autocmds`/
//! `is_autocmd_blocked` (a simple depth-counter, self-contained aside
//! from `unblock_autocmds`'s own "trigger the deferred termresponse
//! autocmd now" branch, which is `unimplemented!()` - nothing
//! currently translated can ever set `TERMRESPONSE_CHANGED` true,
//! since that needs real terminal I/O detecting a termcap response
//! (`tui/*.c`, not yet translated) - provably unreachable today,
//! matching the same "narrow, provably-unreachable branch" precedent
//! used elsewhere in this crate). Also [`augroup_find`]/
//! [`augroup_exists`] - the augroup name-to-ID lookup, via a new
//! `MAP_AUGROUP_NAME_TO_ID` registry that starts empty (matching the
//! original's own `map_augroup_name_to_id`, only ever populated by
//! `:augroup {name}`'s definition side, not yet translated) - so
//! `augroup_find` always returns [`crate::autocmd_defs::augroup::ERROR`]
//! and `augroup_exists` always `false` today, the same "always-empty-
//! registry" pattern as `AUTOCMDS` itself. Also [`event_nr2name`] - a
//! pure lookup using `EventT`'s own derived `Debug` impl in place of
//! the original's `event_names[]` string table (see its own doc
//! comment for why this is a faithful, not approximate, translation).
//!
//! Also translated: [`do_filetype_autocmd`] - fires the `FileType`
//! autocmd event, called from `option.c`'s (still-untranslated)
//! `did_set_option` tail when `'filetype'` changes, and from
//! `api/options.c` (also untranslated). Needed only already-real
//! `GLOBALS.secure` and `BufT::b_did_filetype`/`b_p_ft`/`b_fname`,
//! plus this module's own already-real [`apply_autocmds`]. Always
//! returns `false` today (the identical `AUTOCMDS[EventT::FileType]`-
//! always-empty bypass-path reasoning as every other function in this
//! module), but its own recursion-depth bookkeeping (`FT_RECURSIVE`)
//! and `secure`/`b_did_filetype` side effects are real and correct
//! right now, becoming fully correct end-to-end the moment a future
//! session adds real `FileType` autocmd registration.
//!
//! Deferred: everything else - `apply_autocmds_group`'s real
//! autocmd-matching-and-execution body (needs pattern matching,
//! `exec_autocmds`, script/function invocation via the not-yet-started
//! parser), and the entire `:autocmd`/augroup definition/deletion
//! side (needs the `:autocmd` command parser) - `augroup_name`/
//! `do_augroup` themselves still need real `NEXT_AUGROUP_ID`/
//! `CURRENT_AUGROUP` state tracking beyond just the name-to-ID map.

use crate::autocmd_defs::{augroup, AutoCmdVec, AutoPatCmd, EventT, NUM_EVENTS};
use crate::buffer_defs::BufT;
use crate::ex_cmds_defs::ExargT;
use crate::globals::GlobalCell;
use crate::vim_defs::FAIL;
use std::sync::LazyLock;

/// `autocmds[NUM_EVENTS]` - every event's own vector of registered
/// autocmds, all always empty today; see this module's own doc
/// comment for why that emptiness is exploited (not worked around)
/// to make the functions below correct.
static AUTOCMDS: LazyLock<GlobalCell<[AutoCmdVec; NUM_EVENTS]>> =
    LazyLock::new(|| GlobalCell::new(std::array::from_fn(|_| Vec::new())));

/// `autocmd_busy` (`autocmd.h`) - is `apply_autocmds()` busy? A real
/// cross-file `EXTERN` global (unlike `AU_NEED_CLEAN`/
/// `ACTIVE_APC_LIST`, which are file-static in the original), so
/// `pub` here matching this crate's "each translated globals bag
/// lives in the Rust module matching its own original header"
/// convention (e.g. `mark.h`'s `namedfm` living in `mark.rs`).
///
/// Starts `false`; only ever set `true` inside
/// [`apply_autocmds_group`]'s own real (still-`unimplemented!()`,
/// never-reached-today) autocmd-execution body - so stays `false`
/// forever in practice today, exactly like `AUTOCMDS` staying empty
/// forever. This is what makes `change.c`'s `change_warning` (this
/// crate's first real reader of this global) tractable today.
pub static AUTOCMD_BUSY: GlobalCell<bool> = GlobalCell::new(false);

/// The window the last `CursorMoved` event was reported for
/// (`last_cursormoved_win`).
pub static LAST_CURSORMOVED_WIN: GlobalCell<*mut crate::buffer_defs::WinT> =
    GlobalCell::new(std::ptr::null_mut());

/// The cursor position the last `CursorMoved` event was reported for
/// (`last_cursormoved`). Only meaningful while
/// [`LAST_CURSORMOVED_WIN`] equals `curwin`.
pub static LAST_CURSORMOVED: GlobalCell<crate::pos_defs::PosT> =
    GlobalCell::new(crate::pos_defs::PosT { lnum: 0, col: 0, coladd: 0 });

/// `au_need_clean` - whether [`au_cleanup`] has real work to do.
/// Starts `false`; only ever set by code inside [`aubuflocal_remove`]'s
/// own (always zero-iteration today) loop, so stays `false` forever
/// in practice - matching the original's own file-static.
static AU_NEED_CLEAN: GlobalCell<bool> = GlobalCell::new(false);

/// `active_apc_list` - stack of active autocommands (a singly-linked
/// list via `AutoPatCmd.next`), always null today since nothing can
/// currently execute a real autocmd to push onto it - matching the
/// original's own file-static.
static ACTIVE_APC_LIST: GlobalCell<*mut AutoPatCmd> = GlobalCell::new(std::ptr::null_mut());

/// `autocmd_blocked` - depth counter for [`block_autocmds`]/
/// [`unblock_autocmds`].
static AUTOCMD_BLOCKED: GlobalCell<i32> = GlobalCell::new(0);

/// `termresponse_changed` - whether `v:termresponse` was set while
/// autocommands were blocked. Always `false` today: nothing currently
/// translated can detect/set a real terminal response (needs
/// `tui/*.c`'s terminal I/O, not yet translated) - matching the
/// original's own file-static.
static TERMRESPONSE_CHANGED: GlobalCell<bool> = GlobalCell::new(false);

/// `termresponse_chan_id` - channel that sent the pending terminal
/// response, paired with [`TERMRESPONSE_CHANGED`].
static TERMRESPONSE_CHAN_ID: GlobalCell<u64> = GlobalCell::new(0);

/// Return `true` if `event` autocommand is defined (`has_event`).
#[must_use]
pub fn has_event(event: EventT) -> bool {
    !(unsafe { AUTOCMDS.get_mut() })[event as usize].is_empty()
}

/// Whether a `CursorHold`/`CursorHoldI` autocommand is defined for
/// the mode the editor is really in (`has_cursorhold`).
///
/// Normal mode uses `CursorHold`; anything else (Insert, in
/// particular) uses `CursorHoldI`. The check goes through
/// [`crate::state::get_real_state`] rather than `State` directly, so
/// a pending operator or mapping does not disguise the mode.
#[must_use]
pub fn has_cursorhold() -> bool {
    let normal_busy = crate::state::get_real_state()
        == crate::state_defs::mode::NORMAL_BUSY as i32;
    has_event(if normal_busy { EventT::CursorHold } else { EventT::CursorHoldI })
}

/// Block executing autocommands until [`unblock_autocmds`] is called
/// the same number of times (`block_autocmds`).
pub fn block_autocmds() {
    // Detect if v:termresponse is set while blocked.
    if !is_autocmd_blocked() {
        unsafe { *TERMRESPONSE_CHANGED.get_mut() = false };
        unsafe { *TERMRESPONSE_CHAN_ID.get_mut() = 0 };
    }
    unsafe { *AUTOCMD_BLOCKED.get_mut() += 1 };
}

/// Undo the effect of [`block_autocmds`] (`unblock_autocmds`).
///
/// The original's "trigger the deferred termresponse autocmd now"
/// branch (reached only when `v:termresponse` was set while blocked)
/// is `unimplemented!()` here: nothing currently translated can ever
/// set `TERMRESPONSE_CHANGED` true (needs real terminal I/O
/// detecting a termcap response, `tui/*.c`, not yet translated) - this
/// branch is therefore provably unreachable today, matching the
/// established "narrow, provably-unreachable branch" precedent (e.g.
/// `func_clear_items`'s `FC_LUAREF` arm).
pub fn unblock_autocmds() {
    unsafe { *AUTOCMD_BLOCKED.get_mut() -= 1 };

    // When v:termresponse was set while autocommands were blocked,
    // trigger the autocommands now.
    if !is_autocmd_blocked()
        && unsafe { *TERMRESPONSE_CHANGED.get_mut() }
        && has_event(EventT::TermResponse)
    {
        unimplemented!(
            "unblock_autocmds's deferred termresponse-autocmd trigger: unreachable today, \
             nothing can set TERMRESPONSE_CHANGED true without real terminal I/O (tui/*.c)"
        );
    }
}

/// Return `true` if autocommands are currently blocked
/// (`is_autocmd_blocked`).
#[must_use]
pub fn is_autocmd_blocked() -> bool {
    unsafe { *AUTOCMD_BLOCKED.get_mut() != 0 }
}

/// Execute autocommands for `event` and file name `fname`
/// (`apply_autocmds`).
///
/// Returns `true` if some commands were executed.
#[must_use]
pub fn apply_autocmds(
    event: EventT,
    fname: Option<&[u8]>,
    fname_io: Option<&[u8]>,
    force: bool,
    buf: Option<&BufT>,
) -> bool {
    apply_autocmds_group(
        event,
        fname,
        fname_io,
        force,
        crate::autocmd_defs::augroup::ALL,
        buf,
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        false,
    )
}

/// Like [`apply_autocmds`], but with an extra `eap` argument. This
/// takes care of setting `v:filearg` (in the still-`unimplemented!()`
/// real body) (`apply_autocmds_exarg`).
///
/// Returns `true` if some commands were executed.
#[must_use]
pub fn apply_autocmds_exarg(
    event: EventT,
    fname: Option<&[u8]>,
    fname_io: Option<&[u8]>,
    force: bool,
    buf: Option<&BufT>,
    eap: *mut ExargT,
) -> bool {
    apply_autocmds_group(
        event,
        fname,
        fname_io,
        force,
        crate::autocmd_defs::augroup::ALL,
        buf,
        eap,
        std::ptr::null_mut(),
        false,
    )
}

/// Like [`apply_autocmds`], but handles the caller's `retval`. If the
/// script processing is being aborted or if `retval` is `FAIL` when
/// inside a try conditional, no autocommands are executed. If
/// otherwise the autocommands cause the script to be aborted, `retval`
/// is set to `FAIL` (`apply_autocmds_retval`).
///
/// Returns `true` if some autocommands were executed.
#[must_use]
pub fn apply_autocmds_retval(
    event: EventT,
    fname: Option<&[u8]>,
    fname_io: Option<&[u8]>,
    force: bool,
    buf: Option<&BufT>,
    retval: &mut i32,
) -> bool {
    if crate::ex_eval::should_abort(*retval) {
        return false;
    }

    let did_cmd = apply_autocmds_group(
        event,
        fname,
        fname_io,
        force,
        crate::autocmd_defs::augroup::ALL,
        buf,
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        false,
    );
    if did_cmd && crate::ex_eval::aborting() {
        *retval = FAIL;
    }
    did_cmd
}

/// Execute autocommands for `event` and file name `fname`
/// (`apply_autocmds_group`).
///
/// Returns `true` if some commands were executed. See this module's
/// own doc comment for why only the "no matching autocmds" early-return
/// path is translated - the real matching-and-execution body is
/// `unimplemented!()`, and (per this module's own doc comment)
/// unreachable in practice today.
///
/// `fname`/`fname_io`/`eap`/`data`/`with_buf` are accepted (matching
/// the original's full signature, for forward-compatibility with the
/// real body once it exists) but genuinely unused by the bypass path
/// translated so far. 9 parameters, matching the original's own
/// signature exactly - `#[allow(...)]`ed rather than restructured,
/// since a faithful translation should not invent a parameter-object
/// redesign the original doesn't have.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn apply_autocmds_group(
    event: EventT,
    _fname: Option<&[u8]>,
    _fname_io: Option<&[u8]>,
    _force: bool,
    _group: i32,
    buf: Option<&BufT>,
    _eap: *mut ExargT,
    _data: *mut crate::api::private::defs::Object,
    _with_buf: bool,
) -> bool {
    let retval = false;

    // Quickly return if there are no autocommands for this event or
    // autocommands are blocked. `is_autocmd_blocked()` is never
    // reached: `AUTOCMDS[event].is_empty()` is always true today (see
    // this module's own doc comment), short-circuiting the `||`
    // before it - so it isn't translated here.
    if (event as usize) == NUM_EVENTS
        || (unsafe { AUTOCMDS.get_mut() })[event as usize].is_empty()
    {
        // BYPASS_AU:
        // When wiping out a buffer make sure all its buffer-local
        // autocommands are deleted.
        if event == EventT::BufWipeout
            && let Some(buf) = buf
        {
            unsafe { aubuflocal_remove(buf) };
        }

        // `retval == OK && event == EVENT_FILETYPE` omitted: `retval`
        // is provably `false` throughout this bypass-only path (see
        // this module's own doc comment) - there is no runtime
        // condition under which this branch could fire yet.

        crate::context::ctx_restore(&crate::context_defs::CtxSwitch::default());

        return retval;
    }

    unimplemented!(
        "apply_autocmds_group: the real autocmd-matching-and-execution body needs pattern \
         matching and script/function invocation, not yet translated - unreachable in \
         practice today since AUTOCMDS is always empty, see this module's own doc comment"
    );
}

/// Called when a buffer is freed, to remove/invalidate related
/// buffer-local autocmds (`aubuflocal_remove`).
///
/// Both of this function's own loops are always zero-iteration today
/// (`ACTIVE_APC_LIST` is always null; every `AUTOCMDS[event]` is
/// always empty) - see this module's own doc comment. Faithfully
/// translated anyway (not hardcoded to a no-op), so this starts
/// working correctly the moment a future session makes either
/// precondition false.
///
/// # Safety
/// `ACTIVE_APC_LIST`'s chain (if non-empty) must consist of valid
/// `AutoPatCmd` pointers - always upheld today since the list is
/// always null (see this module's own doc comment); this function
/// stays `unsafe` for when that stops being true.
pub unsafe fn aubuflocal_remove(buf: &BufT) {
    // invalidate currently executing autocommands
    let mut apc = unsafe { *ACTIVE_APC_LIST.get_mut() };
    while !apc.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe {
            if buf.handle == (*apc).arg_bufnr {
                (*apc).arg_bufnr = 0;
            }
            apc = (*apc).next;
        }
    }

    // invalidate buflocals looping through events
    let autocmds = unsafe { AUTOCMDS.get_mut() };
    for acs in autocmds.iter() {
        for ac in acs {
            // SAFETY: forwarded from this function's own safety doc.
            let pat = ac.pat;
            if pat.is_null() || unsafe { (*pat).buflocal_nr } != buf.handle {
                continue;
            }
            unimplemented!(
                "aubuflocal_remove: aucmd_del/verbose messaging needed for a real buffer-local \
                 autocmd match, not yet translated - unreachable in practice today since \
                 AUTOCMDS is always empty, see this module's own doc comment"
            );
        }
    }
    au_cleanup();
}

/// Cleanup autocommands that have been deleted. This is only done
/// when not executing autocommands (`au_cleanup`).
///
/// Always a real no-op today: [`AUTOCMD_BUSY`] and `AU_NEED_CLEAN`
/// both start `false`, and `AU_NEED_CLEAN` is only ever set by code
/// inside [`aubuflocal_remove`]'s own always-zero-iteration loop - see
/// this module's own doc comment.
fn au_cleanup() {
    if unsafe { *AUTOCMD_BUSY.get_mut() } || !unsafe { *AU_NEED_CLEAN.get_mut() } {
        return;
    }
    unimplemented!(
        "au_cleanup: real cleanup needs at least one AUTOCMDS[event] entry to exist, \
         unreachable in practice today - see this module's own doc comment"
    );
}

/// `map_augroup_name_to_id` - augroup name -> ID registry. Always
/// empty today: nothing yet populates it (`:augroup {name}`'s
/// definition side isn't translated).
static MAP_AUGROUP_NAME_TO_ID: LazyLock<GlobalCell<std::collections::HashMap<Vec<u8>, i32>>> =
    LazyLock::new(|| GlobalCell::new(std::collections::HashMap::new()));

/// Find the ID of an autocmd group name, or
/// [`crate::autocmd_defs::augroup::ERROR`] if not found
/// (`augroup_find`).
///
/// Always returns [`crate::autocmd_defs::augroup::ERROR`] today: see
/// this module's own doc comment for why `MAP_AUGROUP_NAME_TO_ID` can
/// only currently be empty.
#[must_use]
pub fn augroup_find(name: &[u8]) -> i32 {
    // SAFETY: momentary read.
    let existing_id = unsafe { MAP_AUGROUP_NAME_TO_ID.get_mut() }.get(name).copied();
    match existing_id {
        Some(id) if id == augroup::DELETED => augroup::DELETED,
        Some(id) if id > 0 => id,
        _ => augroup::ERROR,
    }
}

/// Whether augroup `name` exists (`augroup_exists`).
///
/// Always `false` today - see [`augroup_find`]'s own doc comment.
#[must_use]
pub fn augroup_exists(name: &[u8]) -> bool {
    augroup_find(name) > 0
}

/// The name of `event` (`event_nr2name`).
///
/// Looks the name up in [`crate::autocmd_defs::EVENT_NAMES`], the
/// transcription of the original's own `event_names[]` table, rather
/// than deriving it from the enum's `Debug` formatting as this used
/// to. The two agree today - the `EventT` variants are themselves a
/// mechanical transcription of those same name strings - but relying
/// on that was a coincidence of the derive, not a guarantee; the
/// table is the same source of truth the original reads.
///
/// The original returns `"Unknown"` for an out-of-range event. That
/// cannot happen here, since `EventT` is an enum rather than a raw
/// integer, so every value indexes a real entry.
#[must_use]
pub fn event_nr2name(event: EventT) -> String {
    String::from_utf8_lossy(crate::autocmd_defs::EVENT_NAMES[event as usize].name).into_owned()
}

/// Defer sending the `OptionSet` autocommand for `buf`'s own
/// "modified" option changing until back at the main loop
/// (`aucmd_defer_modified`).
///
/// Since `has_event(EVENT_OPTIONSET)` is always `false` today (nothing
/// in this crate can register a real autocommand yet - see this
/// module's own doc comment), this function's real body is ALWAYS a
/// no-op via its own first, short-circuiting `||` disjunct - the real,
/// always-taken early-return condition is translated directly here,
/// not a hardcoded shortcut, matching this crate's own established
/// "no autocmds registered" bypass-path precedent
/// (`apply_autocmds_group`'s own doc comment). If
/// `has_event(EVENT_OPTIONSET)` were ever real (a future session
/// adding `:autocmd` registration), reaching past it needs
/// `bt_nofile`/`get_vim_var_str`/the deferred-events multiqueue - none
/// of which are wired up here yet, so that path `unimplemented!()`s,
/// unreachable today.
///
/// # Safety
/// `buf` must be a valid, non-null pointer to a live `BufT`.
pub unsafe fn aucmd_defer_modified(buf: *mut BufT, _new_val: bool) {
    if !has_event(EventT::OptionSet) {
        return;
    }
    let _ = buf;
    unimplemented!(
        "aucmd_defer_modified: needs bt_nofile/get_vim_var_str/the deferred-events \
         multiqueue - unreachable while has_event(EVENT_OPTIONSET) is always false"
    );
}

/// Recursion depth guard for [`do_filetype_autocmd`] (`ft_recursive`,
/// a function-local `static int` in the original).
static FT_RECURSIVE: crate::globals::GlobalCell<i32> = crate::globals::GlobalCell::new(0);

/// Fire the `FileType` autocmd event for `buf` (`do_filetype_autocmd`).
///
/// Returns whether any `FileType` autocommands were executed - always
/// `false` today, since `AUTOCMDS[EventT::FileType]` is always empty
/// (nothing in this crate can register a real autocmd yet, matching
/// [`apply_autocmds_group`]'s own established bypass-path precedent).
/// Temporarily resets `GLOBALS.secure` to `0` for the duration of the
/// call (the value of `'filetype'` has already been checked safe by
/// this point in the original's own caller), sets
/// `buf.b_did_filetype`, and fires the event via the already-real
/// [`apply_autocmds`] - becomes fully correct automatically once a
/// future session adds real `FileType` autocmd registration, with no
/// changes needed here.
///
/// # Safety
/// `buf` must be a valid, non-null pointer to a live [`BufT`].
pub unsafe fn do_filetype_autocmd(buf: *mut BufT, force: bool) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let ft_recursive = unsafe { FT_RECURSIVE.get_mut() };
    if *ft_recursive > 0 && !force {
        return false; // disallow recursion
    }

    // SAFETY: momentary read/write, no aliasing.
    let secure_save = unsafe { crate::globals::GLOBALS.get_mut() }.secure;
    // Reset the secure flag, since the value of 'filetype' has been
    // checked to be safe.
    unsafe { crate::globals::GLOBALS.get_mut() }.secure = 0;

    *ft_recursive += 1;
    let force_or_recursive = force || *ft_recursive == 1;

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*buf).b_did_filetype = true };
    // SAFETY: forwarded from this function's own safety doc; a shared
    // reference for the several field reads below.
    let b = unsafe { &*buf };
    let ret = apply_autocmds(EventT::FileType, b.b_p_ft.as_deref(), b.b_fname.as_deref(), force_or_recursive, Some(b));

    // SAFETY: forwarded from this function's own safety doc.
    *unsafe { FT_RECURSIVE.get_mut() } -= 1;
    unsafe { crate::globals::GLOBALS.get_mut() }.secure = secure_save;
    ret
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_event_is_false_for_every_event_when_autocmds_are_always_empty() {
        // AUTOCMDS is a shared GlobalCell - a sibling test elsewhere in
        // this module temporarily populates AUTOCMDS[BufEnter] (under
        // this same lock) to prove has_event's own "found" branch, so
        // this test must hold the lock too, or it can observe that
        // sibling's mid-flight mutation under parallel execution.
        let _lock = crate::globals::global_state_test_lock();
        assert!(!has_event(EventT::BufEnter));
        assert!(!has_event(EventT::VimEnter));
        assert!(!has_event(EventT::WinScrolled));
    }

    #[test]
    fn apply_autocmds_returns_false_when_no_autocmds_registered() {
        assert!(!apply_autocmds(EventT::BufEnter, None, None, false, None));
    }

    #[test]
    fn apply_autocmds_exarg_returns_false_when_no_autocmds_registered() {
        assert!(!apply_autocmds_exarg(
            EventT::BufWritePre,
            None,
            None,
            false,
            None,
            std::ptr::null_mut()
        ));
    }

    #[test]
    fn apply_autocmds_retval_returns_false_and_leaves_retval_unchanged() {
        use crate::vim_defs::OK;
        let mut retval = OK;
        let did_cmd =
            apply_autocmds_retval(EventT::BufEnter, None, None, false, None, &mut retval);
        assert!(!did_cmd);
        assert_eq!(retval, OK);
    }

    #[test]
    fn apply_autocmds_retval_short_circuits_when_retval_already_fail_and_trying() {
        let _lock = crate::globals::global_state_test_lock();
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_trylevel = globals.trylevel;
        let prev_emsg_silent = globals.emsg_silent;
        globals.trylevel = 1;
        globals.emsg_silent = 0;

        let mut retval = FAIL;
        let did_cmd =
            apply_autocmds_retval(EventT::BufEnter, None, None, false, None, &mut retval);
        assert!(!did_cmd);
        assert_eq!(retval, FAIL);

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        globals.trylevel = prev_trylevel;
        globals.emsg_silent = prev_emsg_silent;
    }

    #[test]
    fn apply_autocmds_group_bypass_returns_false() {
        assert!(!apply_autocmds_group(
            EventT::BufEnter,
            None,
            None,
            false,
            crate::autocmd_defs::augroup::ALL,
            None,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            false
        ));
    }

    #[test]
    fn apply_autocmds_group_bufwipeout_with_null_buf_is_safe() {
        // event == BufWipeout but buf is None - aubuflocal_remove must
        // NOT be called (matches the original's own `buf != NULL` guard).
        assert!(!apply_autocmds_group(
            EventT::BufWipeout,
            None,
            None,
            false,
            crate::autocmd_defs::augroup::ALL,
            None,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            false
        ));
    }

    #[test]
    fn aubuflocal_remove_is_a_noop_with_empty_autocmds_and_apc_list() {
        let buf = BufT { handle: 42, ..Default::default() };
        unsafe { aubuflocal_remove(&buf) };
        // No panic, no observable change - both loops are genuinely
        // zero-iteration.
    }

    #[test]
    fn au_cleanup_is_a_noop_when_au_need_clean_is_false() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { *AUTOCMD_BUSY.get_mut() = false };
        unsafe { *AU_NEED_CLEAN.get_mut() = false };
        au_cleanup(); // must not panic
    }

    #[test]
    fn au_cleanup_is_a_noop_when_autocmd_busy_even_if_au_need_clean_is_true() {
        // Not achievable via any real translated function yet (nothing
        // can set AUTOCMD_BUSY true) - pokes it directly to prove
        // au_cleanup's own `autocmd_busy || !au_need_clean` short-circuit
        // is faithfully translated, independent of how AUTOCMD_BUSY
        // eventually gets set.
        let _lock = crate::globals::global_state_test_lock();
        unsafe { *AUTOCMD_BUSY.get_mut() = true };
        unsafe { *AU_NEED_CLEAN.get_mut() = true };
        au_cleanup(); // must not panic (autocmd_busy short-circuits first)
        unsafe { *AUTOCMD_BUSY.get_mut() = false };
        unsafe { *AU_NEED_CLEAN.get_mut() = false };
    }

    #[test]
    fn has_event_reflects_a_manually_populated_autocmds_entry() {
        // Not achievable via any real translated function yet (no
        // :autocmd definition parser exists) - this test pokes AUTOCMDS
        // directly to prove has_event's own check logic is correct,
        // independent of how the vector eventually gets populated.
        let _lock = crate::globals::global_state_test_lock();
        let autocmds = unsafe { AUTOCMDS.get_mut() };
        assert!(autocmds[EventT::BufEnter as usize].is_empty());
        autocmds[EventT::BufEnter as usize].push(crate::autocmd_defs::AutoCmd {
            pat: std::ptr::null_mut(),
            id: 1,
            desc: None,
            handler_cmd: None,
            handler_fn: crate::eval::typval_defs::Callback::default(),
            script_ctx: crate::eval::typval_defs::SctxT::default(),
            once: false,
            nested: false,
        });
        assert!(has_event(EventT::BufEnter));
        // Clean up so other tests sharing this GlobalCell see an empty
        // state again.
        (unsafe { AUTOCMDS.get_mut() })[EventT::BufEnter as usize].clear();
    }

    #[test]
    fn is_autocmd_blocked_false_by_default() {
        let _lock = crate::globals::global_state_test_lock();
        assert!(!is_autocmd_blocked());
    }

    // --- has_cursorhold ---

    /// Pushes a placeholder autocommand for `event`, restoring the
    /// (empty) state on drop.
    struct EventGuard {
        event: EventT,
    }

    impl EventGuard {
        fn set(event: EventT) -> Self {
            (unsafe { AUTOCMDS.get_mut() })[event as usize].push(crate::autocmd_defs::AutoCmd {
                pat: std::ptr::null_mut(),
                id: 1,
                desc: None,
                handler_cmd: None,
                handler_fn: crate::eval::typval_defs::Callback::default(),
                script_ctx: crate::eval::typval_defs::SctxT::default(),
                once: false,
                nested: false,
            });
            Self { event }
        }
    }

    impl Drop for EventGuard {
        fn drop(&mut self) {
            (unsafe { AUTOCMDS.get_mut() })[self.event as usize].clear();
        }
    }

    #[test]
    fn has_cursorhold_picks_cursorhold_in_normal_busy_mode() {
        let _lock = crate::globals::global_state_test_lock();
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev = g.State;
        g.State = crate::state_defs::mode::NORMAL_BUSY as i32;

        {
            let _e = EventGuard::set(EventT::CursorHold);
            assert!(has_cursorhold(), "the Normal-mode event is consulted");
        }
        {
            // The Insert-mode event must NOT satisfy Normal mode.
            let _e = EventGuard::set(EventT::CursorHoldI);
            assert!(!has_cursorhold());
        }

        unsafe { crate::globals::GLOBALS.get_mut() }.State = prev;
    }

    #[test]
    fn has_cursorhold_picks_cursorholdi_outside_normal_busy_mode() {
        let _lock = crate::globals::global_state_test_lock();
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev = g.State;
        g.State = crate::state_defs::mode::INSERT as i32;

        {
            let _e = EventGuard::set(EventT::CursorHoldI);
            assert!(has_cursorhold());
        }
        {
            let _e = EventGuard::set(EventT::CursorHold);
            assert!(!has_cursorhold(), "the Normal-mode event does not apply");
        }

        unsafe { crate::globals::GLOBALS.get_mut() }.State = prev;
    }

    #[test]
    fn has_cursorhold_is_false_with_no_autocommand_defined() {
        let _lock = crate::globals::global_state_test_lock();
        assert!(!has_cursorhold());
    }

    #[test]
    fn block_then_unblock_autocmds_is_balanced() {
        let _lock = crate::globals::global_state_test_lock();
        assert!(!is_autocmd_blocked());
        block_autocmds();
        assert!(is_autocmd_blocked());
        unblock_autocmds();
        assert!(!is_autocmd_blocked());
    }

    #[test]
    fn block_autocmds_nests_correctly() {
        let _lock = crate::globals::global_state_test_lock();
        block_autocmds();
        block_autocmds();
        assert!(is_autocmd_blocked());
        unblock_autocmds();
        assert!(is_autocmd_blocked()); // still blocked - one level remains
        unblock_autocmds();
        assert!(!is_autocmd_blocked());
    }

    #[test]
    fn block_autocmds_resets_termresponse_state_only_on_the_outermost_block() {
        let _lock = crate::globals::global_state_test_lock();
        // Poke TERMRESPONSE_CHANGED/CHAN_ID directly (nothing real can
        // set these yet) to verify block_autocmds's own "only reset on
        // the outermost call" guard - matches the original's own
        // `if (!is_autocmd_blocked())` check preceding the increment.
        unsafe { *TERMRESPONSE_CHANGED.get_mut() = true };
        unsafe { *TERMRESPONSE_CHAN_ID.get_mut() = 42 };

        block_autocmds(); // outermost: resets both to false/0
        assert!(!unsafe { *TERMRESPONSE_CHANGED.get_mut() });
        assert_eq!(unsafe { *TERMRESPONSE_CHAN_ID.get_mut() }, 0);

        // Simulate re-detecting a response while still blocked (nested
        // call must NOT reset it again).
        unsafe { *TERMRESPONSE_CHANGED.get_mut() = true };
        block_autocmds();
        assert!(unsafe { *TERMRESPONSE_CHANGED.get_mut() }); // untouched - not outermost

        unblock_autocmds();
        // Still blocked (1 level remains) - is_autocmd_blocked() is
        // true, so the deferred-trigger branch's own guard condition
        // isn't even reached yet.
        assert!(is_autocmd_blocked());

        // Reset TERMRESPONSE_CHANGED back to false before the final
        // unblock_autocmds() call, else its own deferred-trigger branch
        // (has_event(TermResponse) is also always false today, so the
        // full condition is still false either way, but keep this
        // explicit for clarity and to avoid ever exercising the
        // unimplemented!() branch even by accident).
        unsafe { *TERMRESPONSE_CHANGED.get_mut() = false };
        unblock_autocmds();
        assert!(!is_autocmd_blocked());
    }

    // --- augroup_find / augroup_exists ---

    fn reset_augroup_map() {
        unsafe { MAP_AUGROUP_NAME_TO_ID.get_mut() }.clear();
    }

    #[test]
    fn augroup_find_not_found_is_error() {
        let _lock = crate::globals::global_state_test_lock();
        reset_augroup_map();
        assert_eq!(augroup_find(b"nonexistent"), augroup::ERROR);
        assert!(!augroup_exists(b"nonexistent"));
    }

    #[test]
    fn augroup_find_returns_a_real_positive_id() {
        let _lock = crate::globals::global_state_test_lock();
        reset_augroup_map();
        unsafe { MAP_AUGROUP_NAME_TO_ID.get_mut() }.insert(b"MyGroup".to_vec(), 5);
        assert_eq!(augroup_find(b"MyGroup"), 5);
        assert!(augroup_exists(b"MyGroup"));
        reset_augroup_map();
    }

    #[test]
    fn augroup_find_returns_deleted_for_a_deleted_group() {
        let _lock = crate::globals::global_state_test_lock();
        reset_augroup_map();
        unsafe { MAP_AUGROUP_NAME_TO_ID.get_mut() }.insert(b"WasDeleted".to_vec(), augroup::DELETED);
        assert_eq!(augroup_find(b"WasDeleted"), augroup::DELETED);
        assert!(!augroup_exists(b"WasDeleted"), "a deleted group does not count as existing");
        reset_augroup_map();
    }

    #[test]
    fn augroup_find_distinguishes_between_multiple_entries() {
        let _lock = crate::globals::global_state_test_lock();
        reset_augroup_map();
        unsafe { MAP_AUGROUP_NAME_TO_ID.get_mut() }.insert(b"GroupA".to_vec(), 1);
        unsafe { MAP_AUGROUP_NAME_TO_ID.get_mut() }.insert(b"GroupB".to_vec(), 2);
        assert_eq!(augroup_find(b"GroupA"), 1);
        assert_eq!(augroup_find(b"GroupB"), 2);
        assert_eq!(augroup_find(b"GroupC"), augroup::ERROR);
        reset_augroup_map();
    }

    // --- event_nr2name ---

    #[test]
    fn event_nr2name_first_variant() {
        assert_eq!(event_nr2name(EventT::BufAdd), "BufAdd");
    }

    #[test]
    fn event_nr2name_last_variant() {
        assert_eq!(event_nr2name(EventT::WinScrolled), "WinScrolled");
    }

    #[test]
    fn event_nr2name_an_alias_variant_is_distinct_from_its_target() {
        // BufCreate is a real, distinct EventT variant (an alias of
        // BufAdd, per auevents.lua's own "aliases" table) - its own
        // display name must be "BufCreate", not "BufAdd".
        assert_eq!(event_nr2name(EventT::BufCreate), "BufCreate");
        assert_ne!(event_nr2name(EventT::BufCreate), event_nr2name(EventT::BufAdd));
    }

    #[test]
    fn event_nr2name_tricky_capitalizations_match_the_generated_table_exactly() {
        // Hand-verified against auevents_name_map.generated.h's own
        // exact "CmdlineChanged"/"CmdwinEnter" strings (lowercase 'l'/
        // 'w' - NOT "CmdLineChanged"/"CmdWinEnter").
        assert_eq!(event_nr2name(EventT::CmdlineChanged), "CmdlineChanged");
        assert_eq!(event_nr2name(EventT::CmdwinEnter), "CmdwinEnter");
    }

    #[test]
    fn event_nr2name_spot_checks_across_the_enum_produce_plain_ascii_names() {
        // A spread of variants (early/middle/late, and the other 2
        // aliases besides BufCreate) - each hand-verified against
        // auevents_name_map.generated.h's own exact string, and none
        // carrying a stray "EventT::" Debug-path prefix.
        assert_eq!(event_nr2name(EventT::BufRead), "BufRead"); // alias of BufReadPost
        assert_eq!(event_nr2name(EventT::BufReadPost), "BufReadPost");
        assert_eq!(event_nr2name(EventT::VimEnter), "VimEnter");
        assert_eq!(event_nr2name(EventT::TermResponse), "TermResponse");
        assert_eq!(event_nr2name(EventT::WinResized), "WinResized");
        for name in [
            event_nr2name(EventT::BufAdd),
            event_nr2name(EventT::VimEnter),
            event_nr2name(EventT::WinScrolled),
        ] {
            assert!(!name.is_empty());
            assert!(name.is_ascii());
            assert!(!name.contains("::"));
        }
    }

    #[test]
    fn aucmd_defer_modified_is_a_noop_when_no_optionset_autocmd_is_registered() {
        // AUTOCMDS is shared - hold the lock even though this test
        // only reads through has_event's own no-op fast path (a
        // sibling test elsewhere in this module populates
        // AUTOCMDS[BufEnter] under this same lock).
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        // Must not panic - has_event(EVENT_OPTIONSET) is always false
        // today, so the real body's own first `||` disjunct always
        // short-circuits the whole condition to true and returns
        // immediately, never reaching the unimplemented!() tail.
        unsafe { aucmd_defer_modified(&mut buf as *mut BufT, true) };
        unsafe { aucmd_defer_modified(&mut buf as *mut BufT, false) };
    }

    #[test]
    fn aucmd_defer_modified_panics_if_an_optionset_autocmd_is_ever_registered() {
        // A hypothetical future state (never reachable today, since
        // nothing can populate AUTOCMDS[OptionSet] yet) - proves the
        // real early-return condition is genuinely checked, not
        // hardcoded, and that the (today unreachable) tail correctly
        // signals its own remaining blocker rather than silently
        // doing the wrong thing. Checks the panic message manually
        // (not via #[should_panic]) since a manual catch_unwind is
        // needed to safely clean up AUTOCMDS even if the call panics -
        // combining both mechanisms causes #[should_panic] to observe
        // the wrong (re-thrown/unwrap) panic message instead of the
        // real one.
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        {
            let autocmds = unsafe { AUTOCMDS.get_mut() };
            assert!(autocmds[EventT::OptionSet as usize].is_empty());
            autocmds[EventT::OptionSet as usize].push(crate::autocmd_defs::AutoCmd {
                pat: std::ptr::null_mut(),
                id: 1,
                desc: None,
                handler_cmd: None,
                handler_fn: crate::eval::typval_defs::Callback::default(),
                script_ctx: crate::eval::typval_defs::SctxT::default(),
                once: false,
                nested: false,
            });
        }
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
            aucmd_defer_modified(&mut buf as *mut BufT, true);
        }));
        // Clean up so other tests sharing this GlobalCell see an empty
        // state again, regardless of whether the call above panicked.
        (unsafe { AUTOCMDS.get_mut() })[EventT::OptionSet as usize].clear();

        let err = result.expect_err("aucmd_defer_modified should have panicked");
        // unimplemented!() with a plain string literal (no format
        // interpolation) panics with a `&'static str` payload, not a
        // `String`.
        let msg = err.downcast_ref::<&str>().copied().unwrap_or("<non-string panic>");
        assert!(msg.contains("aucmd_defer_modified"), "unexpected panic message: {msg}");
    }

    // --- do_filetype_autocmd ---

    fn reset_ft_recursive() {
        unsafe { *FT_RECURSIVE.get_mut() = 0 };
    }

    #[test]
    fn do_filetype_autocmd_sets_b_did_filetype_and_returns_false_via_the_bypass_path() {
        let _lock = crate::globals::global_state_test_lock();
        reset_ft_recursive();
        let mut buf = BufT::default();
        assert!(!buf.b_did_filetype);

        let ret = unsafe { do_filetype_autocmd(&mut buf as *mut BufT, false) };

        assert!(!ret, "AUTOCMDS[FileType] is always empty - the bypass path never executes anything");
        assert!(buf.b_did_filetype);
        reset_ft_recursive();
    }

    #[test]
    fn do_filetype_autocmd_leaves_ft_recursive_back_at_zero_after_a_normal_call() {
        let _lock = crate::globals::global_state_test_lock();
        reset_ft_recursive();
        let mut buf = BufT::default();
        unsafe { do_filetype_autocmd(&mut buf as *mut BufT, false) };
        assert_eq!(unsafe { *FT_RECURSIVE.get_mut() }, 0, "increment/decrement net to zero");
    }

    #[test]
    fn do_filetype_autocmd_disallows_recursion_when_force_is_false() {
        let _lock = crate::globals::global_state_test_lock();
        // Simulate being already inside a call (as if called
        // recursively from within apply_autocmds' own execution).
        unsafe { *FT_RECURSIVE.get_mut() = 1 };
        let mut buf = BufT::default();

        let ret = unsafe { do_filetype_autocmd(&mut buf as *mut BufT, false) };

        assert!(!ret);
        assert!(!buf.b_did_filetype, "the recursion guard returns before touching b_did_filetype");
        assert_eq!(unsafe { *FT_RECURSIVE.get_mut() }, 1, "guard returns before any incr/decr");
        reset_ft_recursive();
    }

    #[test]
    fn do_filetype_autocmd_force_true_bypasses_the_recursion_guard() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { *FT_RECURSIVE.get_mut() = 1 };
        let mut buf = BufT::default();

        unsafe { do_filetype_autocmd(&mut buf as *mut BufT, true) };

        assert!(buf.b_did_filetype, "force=true bypasses the guard, reaching the real body");
        assert_eq!(unsafe { *FT_RECURSIVE.get_mut() }, 1, "net zero: 1 -> 2 -> 1");
        reset_ft_recursive();
    }

    #[test]
    fn do_filetype_autocmd_restores_the_secure_flag() {
        let _lock = crate::globals::global_state_test_lock();
        reset_ft_recursive();
        let prev_secure = unsafe { crate::globals::GLOBALS.get_mut() }.secure;
        unsafe { crate::globals::GLOBALS.get_mut() }.secure = 1;

        let mut buf = BufT::default();
        unsafe { do_filetype_autocmd(&mut buf as *mut BufT, false) };

        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.secure, 1, "restored after the call");
        unsafe { crate::globals::GLOBALS.get_mut() }.secure = prev_secure;
        reset_ft_recursive();
    }
}



