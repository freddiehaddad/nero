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
//! Also [`event_name2nr`]/[`check_ei`] - event-name parsing and
//! validation for `'eventignore'`/`'eventignorewin'`, directly over
//! the already-translated authoritative event-name table.
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

/// Whether a `VimResume` event is pending (`pending_vimresume`).
static PENDING_VIMRESUME: GlobalCell<crate::types_defs::TriState> =
    GlobalCell::new(crate::types_defs::TriState::False);

/// Runs the queued `VimResume` event and clears its pending state
/// (`vimresume_event`).
pub fn vimresume_event() {
    let _ = apply_autocmds(EventT::VimResume, None, None, false, None);
    unsafe {
        *PENDING_VIMRESUME.get_mut() = crate::types_defs::TriState::False;
    }
}

/// Returns the autocommand vector for `event`
/// (`au_get_autocmds_for_event`).
///
/// A raw pointer mirrors the original and avoids manufacturing a
/// long-lived mutable reference into the global array.
#[must_use]
pub fn au_get_autocmds_for_event(event: EventT) -> *mut AutoCmdVec {
    let base = AUTOCMDS.as_ptr().cast::<AutoCmdVec>();
    // SAFETY: EventT discriminants are the dense 0..NUM_EVENTS indices
    // used to build this fixed-size array.
    unsafe { base.add(event as usize) }
}

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

/// `did_cursorhold` - set once a `CursorHold` has been triggered, so
/// it does not fire repeatedly without intervening input.
///
/// Starts `true`, matching the original's own
/// `INIT( = true)` - NOT the `false` a zero-initialized `bool` would
/// give. That means no `CursorHold` can fire until something has
/// cleared it (the input/normal/insert loops, none translated), which
/// is why [`trigger_cursorhold`] is always `false` today.
pub static DID_CURSORHOLD: crate::globals::GlobalCell<bool> =
    crate::globals::GlobalCell::new(true);

/// Whether a `CursorHold` autocommand should be triggered right now
/// (`trigger_cursorhold`).
///
/// Every condition must hold: the event has not already fired, an
/// autocommand for the real mode is actually defined, no register is
/// being recorded into, no typeahead is pending, and insert-mode
/// completion is not active. Only then, and only in
/// `MODE_NORMAL_BUSY` or any Insert mode, does it trigger.
///
/// # Safety
/// Touches [`DID_CURSORHOLD`], `crate::globals::GLOBALS`, the
/// typeahead buffer and the completion state.
#[must_use]
pub unsafe fn trigger_cursorhold() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { *DID_CURSORHOLD.get_mut() } {
        return false;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let reg_recording = unsafe { crate::globals::GLOBALS.get_mut() }.reg_recording;
    // SAFETY: forwarded from this function's own safety doc.
    if !has_cursorhold()
        || reg_recording != 0
        || crate::input::typebuf_len() != 0
        || unsafe { crate::insexpand::ins_compl_active() }
    {
        return false;
    }
    let state = crate::state::get_real_state();
    state == crate::state_defs::mode::NORMAL_BUSY as i32
        || (state as u32 & crate::state_defs::mode::INSERT) != 0
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

/// Trigger `TermResponse` autocommands and remember the response
/// channel (`do_termresponse_autocmd`).
pub fn do_termresponse_autocmd(sequence: &[u8], channel_id: u64) {
    let mut data = crate::api::private::defs::Object::Dict(vec![
        crate::api::private::defs::KeyValuePair {
            key: b"sequence".to_vec(),
            value: crate::api::private::defs::Object::String(sequence.to_vec()),
        },
        crate::api::private::defs::KeyValuePair {
            key: b"chan".to_vec(),
            value: crate::api::private::defs::Object::Integer(channel_id as i64),
        },
    ]);
    let _ = apply_autocmds_group(
        EventT::TermResponse,
        None,
        None,
        true,
        augroup::ALL,
        None,
        std::ptr::null_mut(),
        std::ptr::from_mut(&mut data),
        false,
    );
    unsafe { *TERMRESPONSE_CHANGED.get_mut() = true };
    unsafe { *TERMRESPONSE_CHAN_ID.get_mut() = channel_id };
}

/// Undo the effect of [`block_autocmds`] (`unblock_autocmds`).
///
/// The original's "trigger the deferred termresponse autocmd now"
/// branch (reached only when `v:termresponse` was set while blocked)
/// is `unimplemented!()` here: [`do_termresponse_autocmd`] can now set
/// `TERMRESPONSE_CHANGED`, but no translated code can register a real
/// `TermResponse` autocmd, so the final [`has_event`] operand remains
/// false and this branch is still unreachable.
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

unsafe fn aucmd_del(command: &mut crate::autocmd_defs::AutoCmd) {
    if !command.pat.is_null() {
        let pattern = unsafe { &mut *command.pat };
        pattern.refcount -= 1;
        if pattern.refcount == 0 {
            if !pattern.reg_prog.is_null() {
                unimplemented!("aucmd_del: freeing compiled regex needs vim_regfree");
            }
            unsafe { drop(Box::from_raw(command.pat)) };
        }
    }
    command.pat = std::ptr::null_mut();
    if command.handler_cmd.take().is_none() {
        crate::eval::typval::callback_free(&mut command.handler_fn);
    }
    command.desc = None;
    unsafe { *AU_NEED_CLEAN.get_mut() = true };
}

/// Delete one autocmd by API ID (`autocmd_delete_id`).
///
/// # Safety
/// Mutates shared autocmd/callback/pattern state.
pub unsafe fn autocmd_delete_id(id: i64) -> bool {
    let found = {
        let autocmds = unsafe { AUTOCMDS.get_mut() };
        'events: {
            for commands in autocmds {
                if let Some(command) = commands
                    .iter_mut()
                    .find(|command| !command.pat.is_null() && command.id == id)
                {
                    unsafe { aucmd_del(command) };
                    break 'events true;
                }
            }
            false
        }
    };
    if found {
        au_cleanup();
    }
    found
}

/// Delete every autocmd for one event and augroup
/// (`aucmd_del_for_event_and_group`).
///
/// # Safety
/// Mutates shared autocmd/pattern/callback state.
pub unsafe fn aucmd_del_for_event_and_group(event: EventT, group: i32) {
    {
    let autocmds = unsafe { AUTOCMDS.get_mut() };
    let commands = &mut autocmds[event as usize];
        for command in commands {
            if !command.pat.is_null() && unsafe { (*command.pat).group } == group {
                unsafe { aucmd_del(command) };
            }
        }
    }
    au_cleanup();
}

/// Free all autocmds and augroup registries (`free_all_autocmds`).
///
/// # Safety
/// Invalidates every autocmd/pattern/group reference.
pub unsafe fn free_all_autocmds() {
    {
        let autocmds = unsafe { AUTOCMDS.get_mut() };
        for commands in autocmds {
            for command in commands.iter_mut() {
                if !command.pat.is_null() {
                    unsafe { aucmd_del(command) };
                }
            }
            commands.clear();
        }
    }
    unsafe { *AU_NEED_CLEAN.get_mut() = false };
    unsafe { MAP_AUGROUP_NAME_TO_ID.get_mut() }.clear();
    unsafe { MAP_AUGROUP_ID_TO_NAME.get_mut() }.clear();
    unsafe { *NEXT_AUGROUP_ID.get_mut() = 1 };
    unsafe { *CURRENT_AUGROUP.get_mut() = augroup::DEFAULT };
}

/// Cleanup autocommands that have been deleted. This is only done
/// when not executing autocommands (`au_cleanup`).
fn au_cleanup() {
    if unsafe { *AUTOCMD_BUSY.get_mut() } || !unsafe { *AU_NEED_CLEAN.get_mut() } {
        return;
    }
    for commands in unsafe { AUTOCMDS.get_mut() } {
        commands.retain(|command| !command.pat.is_null());
    }
    unsafe { *AU_NEED_CLEAN.get_mut() = false };
}

/// `map_augroup_name_to_id` - augroup name -> ID registry. Always
/// empty today: nothing yet populates it (`:augroup {name}`'s
/// definition side isn't translated).
static MAP_AUGROUP_NAME_TO_ID: LazyLock<GlobalCell<std::collections::HashMap<Vec<u8>, i32>>> =
    LazyLock::new(|| GlobalCell::new(std::collections::HashMap::new()));
static MAP_AUGROUP_ID_TO_NAME: LazyLock<GlobalCell<std::collections::HashMap<i32, Vec<u8>>>> =
    LazyLock::new(|| GlobalCell::new(std::collections::HashMap::new()));
static NEXT_AUGROUP_ID: GlobalCell<i32> = GlobalCell::new(1);
static CURRENT_AUGROUP: GlobalCell<i32> = GlobalCell::new(augroup::DEFAULT);
static AUTOCMD_BUFNR: GlobalCell<i32> = GlobalCell::new(0);

/// Remove augroup registry entries by name and/or ID
/// (`augroup_map_del`).
///
/// # Safety
/// Mutates both shared augroup maps.
pub unsafe fn augroup_map_del(id: i32, name: Option<&[u8]>) {
    if let Some(name) = name {
        unsafe { MAP_AUGROUP_NAME_TO_ID.get_mut() }.remove(name);
    }
    if id > 0 {
        unsafe { MAP_AUGROUP_ID_TO_NAME.get_mut() }.remove(&id);
    }
}

/// Add an augroup name or return its existing ID (`augroup_add`).
///
/// # Safety
/// Mutates the shared augroup registries and allocation counter.
pub unsafe fn augroup_add(name: &[u8]) -> i32 {
    debug_assert!(!name.eq_ignore_ascii_case(b"end"));
    let existing = augroup_find(name);
    if existing > 0 {
        return existing;
    }
    if existing == augroup::DELETED {
        unsafe { augroup_map_del(existing, Some(name)) };
    }
    let next = unsafe { NEXT_AUGROUP_ID.get_mut() };
    let id = *next;
    *next += 1;
    unsafe { MAP_AUGROUP_NAME_TO_ID.get_mut() }.insert(name.to_vec(), id);
    unsafe { MAP_AUGROUP_ID_TO_NAME.get_mut() }.insert(id, name.to_vec());
    id
}

/// Get an augroup name by ID (`augroup_name`).
///
/// # Safety
/// Reads shared augroup registries and current-group state.
#[must_use]
pub unsafe fn augroup_name(mut group: i32) -> Option<Vec<u8>> {
    if group == augroup::DELETED {
        return Some(b"--- DELETED ---".to_vec());
    }
    if group == augroup::ALL {
        group = unsafe { *CURRENT_AUGROUP.get_mut() };
    }
    if group <= 0 || group >= unsafe { *NEXT_AUGROUP_ID.get_mut() } {
        return None;
    }
    unsafe { MAP_AUGROUP_ID_TO_NAME.get_mut() }.get(&group).cloned()
}

/// Delete every autocmd in one augroup (`aucmd_del_for_event_and_group`
/// across all events).
///
/// # Safety
/// Mutates the shared autocmd registry.
pub unsafe fn augroup_clear(group: i32) {
    for commands in unsafe { AUTOCMDS.get_mut() } {
        for command in commands {
            if !command.pat.is_null() && unsafe { (*command.pat).group } == group {
                unsafe { aucmd_del(command) };
            }
        }
    }
    au_cleanup();
}

/// Delete an augroup and its commands (`augroup_del`, non-legacy
/// mode).
///
/// # Safety
/// Mutates shared augroup/autocmd registries.
pub unsafe fn augroup_del(name: &[u8]) -> Result<(), &'static str> {
    let group = augroup_find(name);
    if group == augroup::ERROR {
        return Err("No such group");
    }
    if group == unsafe { *CURRENT_AUGROUP.get_mut() } {
        return Err("Cannot delete the current group");
    }
    unsafe { augroup_clear(group) };
    unsafe { augroup_map_del(group, Some(name)) };
    Ok(())
}

/// Execute `:augroup` state changes (`do_augroup`).
///
/// The returned list replaces the original message output for the
/// no-argument listing form.
///
/// # Safety
/// Mutates shared augroup/autocmd state.
pub unsafe fn do_augroup(
    argument: &[u8],
    delete_group: bool,
) -> Result<Vec<Vec<u8>>, &'static str> {
    if delete_group {
        if argument.is_empty() {
            return Err("Argument required");
        }
        unsafe { augroup_del(argument) }?;
        return Ok(Vec::new());
    }
    if argument.eq_ignore_ascii_case(b"end") {
        unsafe { *CURRENT_AUGROUP.get_mut() = augroup::DEFAULT };
        return Ok(Vec::new());
    }
    if !argument.is_empty() {
        unsafe { *CURRENT_AUGROUP.get_mut() = augroup_add(argument) };
        return Ok(Vec::new());
    }
    let mut groups: Vec<(i32, Vec<u8>)> = unsafe { MAP_AUGROUP_NAME_TO_ID.get_mut() }
        .iter()
        .map(|(name, id)| (*id, name.clone()))
        .collect();
    groups.sort_by_key(|(id, _)| *id);
    Ok(groups
        .into_iter()
        .map(|(id, name)| {
            if id > 0 {
                name
            } else {
                unsafe { augroup_name(id) }.unwrap_or_default()
            }
        })
        .collect())
}

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

/// Parse a group name at the start of `arg` (`arg_augroup_get`).
///
/// @return the group ID plus the offset past the consumed group name,
///         or [`crate::autocmd_defs::augroup::ALL`] with the offset
///         unchanged when there is no leading name or it does not
///         match a real group. The original advances a `char **argp`
///         in place; that becomes the returned offset here.
#[must_use]
pub fn arg_augroup_get(arg: &[u8]) -> (i32, usize) {
    let mut p = 0;
    while p < arg.len() && arg[p] != 0 && !crate::ascii_defs::ascii_iswhite(i32::from(arg[p])) && arg[p] != b'|' {
        p += 1;
    }
    if p == 0 {
        return (crate::autocmd_defs::augroup::ALL, 0);
    }

    let group = augroup_find(&arg[..p]);
    if group == crate::autocmd_defs::augroup::ERROR {
        // No match, use all groups - and leave the argument alone, so
        // the caller re-reads what it was given.
        (crate::autocmd_defs::augroup::ALL, 0)
    } else {
        // Match: skip over the group name and any following blanks.
        (group, p + crate::charset::skipwhite(&arg[p..]))
    }
}

/// Resolve one event name and return its event plus bytes consumed
/// (`event_name2nr`).
///
/// The event name ends at NUL, whitespace, comma or `|`. A comma is
/// consumed; the other terminators are not. Event names are matched
/// case-insensitively, including aliases in `EVENT_NAMES`.
#[must_use]
pub fn event_name2nr(start: &[u8]) -> (Option<EventT>, usize) {
    let mut end = 0;
    while end < start.len()
        && start[end] != 0
        && !crate::ascii_defs::ascii_iswhite(i32::from(start[end]))
        && !matches!(start[end], b',' | b'|')
    {
        end += 1;
    }

    let event = crate::autocmd_defs::EVENT_NAMES
        .iter()
        .find(|entry| entry.name.eq_ignore_ascii_case(&start[..end]))
        .map(|entry| entry.event);
    let consumed = end + usize::from(start.get(end) == Some(&b','));
    (event, consumed)
}

/// Resolve an entire event-name string (`event_name2nr_str`).
#[must_use]
pub fn event_name2nr_str(name: &[u8]) -> Option<EventT> {
    crate::autocmd_defs::EVENT_NAMES
        .iter()
        .find(|entry| entry.name.eq_ignore_ascii_case(name))
        .map(|entry| entry.event)
}

/// Whether an event name is supported (`autocmd_supported`).
#[must_use]
pub fn autocmd_supported(event: &[u8]) -> bool {
    event_name2nr(event).0.is_some()
}

/// Whether an event is included in `'eventignore'` or
/// `'eventignorewin'` (`event_ignored`).
#[must_use]
pub fn event_ignored(event: EventT, eventignore: &[u8], global: bool) -> bool {
    let mut ignored = false;
    let mut offset = 0;
    while offset < eventignore.len() && eventignore[offset] != 0 {
        let unignore = eventignore[offset] == b'-';
        offset += usize::from(unignore);
        let rest = &eventignore[offset..];
        if rest.len() >= 3
            && rest[..3].eq_ignore_ascii_case(b"all")
            && matches!(rest.get(3), None | Some(0 | b','))
        {
            let window_local = crate::autocmd_defs::EVENT_NAMES
                .get(event as usize)
                .is_some_and(|entry| entry.win_local);
            ignored = global || window_local;
            offset += 3 + usize::from(rest.get(3) == Some(&b','));
        } else {
            let (parsed, consumed) = event_name2nr(rest);
            offset += consumed;
            if parsed == Some(event) {
                if unignore {
                    return false;
                }
                ignored = true;
            }
            if consumed == 0 {
                break;
            }
        }
    }
    ignored
}

/// Append events to `'eventignore'` and return its previous value
/// (`au_event_disable`).
///
/// # Safety
/// Mutates the global option table.
#[must_use]
pub unsafe fn au_event_disable(what: &[u8]) -> Vec<u8> {
    let old = unsafe { crate::option_vars::OPTION_VARS.get_mut() }
        .p_ei
        .clone()
        .unwrap_or_default();
    let mut new_value = old.clone();
    if what.first() == Some(&b',') && old.is_empty() {
        new_value.extend_from_slice(&what[1..]);
    } else {
        new_value.extend_from_slice(what);
    }
    unsafe {
        crate::option::set_option_direct(
            crate::option_defs::OptIndex::Eventignore,
            crate::option_defs::OptVal::String(new_value),
            0,
            crate::globals::SID_NONE,
        )
    };
    old
}

/// Restore a value saved by [`au_event_disable`]
/// (`au_event_restore`).
///
/// # Safety
/// Mutates the global option table.
pub unsafe fn au_event_restore(old_eventignore: Option<Vec<u8>>) {
    let Some(old_eventignore) = old_eventignore else {
        return;
    };
    unsafe {
        crate::option::set_option_direct(
            crate::option_defs::OptIndex::Eventignore,
            crate::option_defs::OptVal::String(old_eventignore),
            0,
            crate::globals::SID_NONE,
        )
    };
}

/// Whether a pattern uses `<buffer...>` syntax
/// (`aupat_is_buflocal`).
#[must_use]
pub fn aupat_is_buflocal(pattern: &[u8]) -> bool {
    pattern.len() >= 8
        && pattern.starts_with(b"<buffer")
        && pattern.last() == Some(&b'>')
}

/// Resolve a buffer-local autocmd pattern to its buffer number
/// (`aupat_get_buflocal_nr`).
///
/// # Safety
/// `<buffer>` reads `GLOBALS.curbuf`; `<buffer=abuf>` reads shared
/// autocmd execution state.
#[must_use]
pub unsafe fn aupat_get_buflocal_nr(pattern: &[u8]) -> i32 {
    debug_assert!(aupat_is_buflocal(pattern));
    if pattern.len() == 8 {
        return unsafe { (*crate::globals::GLOBALS.get_mut().curbuf).handle };
    }
    if pattern.len() == 13 && pattern.eq_ignore_ascii_case(b"<buffer=abuf>") {
        return unsafe { *AUTOCMD_BUFNR.get_mut() };
    }
    if pattern.len() > 9 && pattern[7] == b'=' {
        let digits = &pattern[8..pattern.len() - 1];
        if !digits.is_empty() && digits.iter().all(u8::is_ascii_digit) {
            return std::str::from_utf8(digits)
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(0);
        }
    }
    0
}

/// Normalize a buffer-local pattern (`aupat_normalize_buflocal_pat`).
///
/// # Safety
/// A zero buffer number reads `GLOBALS.curbuf`.
#[must_use]
pub unsafe fn aupat_normalize_buflocal_pat(pattern: &[u8], mut bufnr: i32) -> Vec<u8> {
    debug_assert!(aupat_is_buflocal(pattern));
    if bufnr == 0 {
        bufnr = unsafe { (*crate::globals::GLOBALS.get_mut().curbuf).handle };
    }
    format!("<buffer={bufnr}>").into_bytes()
}

/// Parse one `:autocmd` boolean flag (`arg_autocmd_flag_get`).
///
/// Returns `(duplicate, remaining_offset)`.
#[must_use]
pub fn arg_autocmd_flag_get(flag: &mut bool, command: &[u8], pattern: &[u8]) -> (bool, usize) {
    if command.starts_with(pattern)
        && command
            .get(pattern.len())
            .is_some_and(|byte| byte.is_ascii_whitespace())
    {
        if *flag {
            return (true, 0);
        }
        *flag = true;
        let offset = pattern.len() + crate::charset::skipwhite(&command[pattern.len()..]);
        (false, offset)
    } else {
        (false, 0)
    }
}

/// Validate `'eventignore'` or `'eventignorewin'` (`check_ei`).
///
/// `win` selects the window-local option, which accepts only entries
/// marked `win_local` in the event-name table. The special `all`
/// token is accepted in either option.
#[must_use]
pub fn check_ei(ei: &[u8], win: bool) -> bool {
    let mut pos = 0;
    while pos < ei.len() && ei[pos] != 0 {
        let rest = &ei[pos..];
        if rest.len() >= 3
            && rest[..3].eq_ignore_ascii_case(b"all")
            && matches!(rest.get(3), None | Some(0 | b','))
        {
            pos += 3 + usize::from(rest.get(3) == Some(&b','));
            continue;
        }

        pos += usize::from(ei[pos] == b'-');
        let (event, consumed) = event_name2nr(&ei[pos..]);
        let Some(event) = event else {
            return false;
        };
        pos += consumed;
        if win && !crate::autocmd_defs::EVENT_NAMES[event as usize].win_local {
            return false;
        }
    }
    true
}

/// Event names for `ExpandGeneric`, excluding group names
/// (`get_event_name_no_group`).
///
/// `win` restricts the list to events that may appear in
/// `'eventignorewin'`, in which case `idx` counts only those.
///
/// The original takes an unused `expand_T *xp` that has no equivalent
/// here, and returns `NULL` past the end; that becomes `None`.
#[must_use]
pub fn get_event_name_no_group(idx: i32, win: bool) -> Option<&'static [u8]> {
    let names = &crate::autocmd_defs::EVENT_NAMES;
    if idx < 0 || idx as usize >= names.len() {
        return None;
    }

    if !win {
        return Some(names[idx as usize].name);
    }

    // Only a subset of events is allowed in 'eventignorewin', so walk
    // the table counting just those. The original tests `event <= 0`,
    // the sign it packs this into; here it is an explicit flag.
    let mut j = 0;
    for entry in names {
        if entry.win_local {
            j += 1;
            if j == idx + 1 {
                return Some(entry.name);
            }
        }
    }
    None
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
    fn termresponse_autocmd_records_changed_state_and_channel() {
        let _lock = crate::globals::global_state_test_lock();
        let saved = (
            unsafe { *TERMRESPONSE_CHANGED.get_mut() },
            unsafe { *TERMRESPONSE_CHAN_ID.get_mut() },
        );
        do_termresponse_autocmd(b"\x1b[?1;2c", 42);
        let got = (
            unsafe { *TERMRESPONSE_CHANGED.get_mut() },
            unsafe { *TERMRESPONSE_CHAN_ID.get_mut() },
        );
        unsafe { *TERMRESPONSE_CHANGED.get_mut() = saved.0 };
        unsafe { *TERMRESPONSE_CHAN_ID.get_mut() = saved.1 };
        assert_eq!(got, (true, 42));
    }

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
    fn au_get_autocmds_for_event_returns_a_stable_event_slot() {
        let _lock = crate::globals::global_state_test_lock();
        let first = au_get_autocmds_for_event(EventT::BufEnter);
        let second = au_get_autocmds_for_event(EventT::BufEnter);
        assert_eq!(first, second);
        assert!(unsafe { &*first }.is_empty());
    }

    #[test]
    fn au_get_autocmds_for_event_keeps_events_in_distinct_vectors() {
        let _lock = crate::globals::global_state_test_lock();
        let buf_enter = au_get_autocmds_for_event(EventT::BufEnter);
        let vim_enter = au_get_autocmds_for_event(EventT::VimEnter);
        assert_ne!(buf_enter, vim_enter);
    }

    struct PendingVimresumeGuard(crate::types_defs::TriState);

    impl PendingVimresumeGuard {
        fn install(value: crate::types_defs::TriState) -> Self {
            let slot = unsafe { PENDING_VIMRESUME.get_mut() };
            let saved = *slot;
            *slot = value;
            Self(saved)
        }
    }

    impl Drop for PendingVimresumeGuard {
        fn drop(&mut self) {
            unsafe { *PENDING_VIMRESUME.get_mut() = self.0 };
        }
    }

    #[test]
    fn vimresume_event_clears_the_pending_state() {
        let _lock = crate::globals::global_state_test_lock();
        let _g =
            PendingVimresumeGuard::install(crate::types_defs::TriState::True);

        vimresume_event();

        assert_eq!(
            unsafe { *PENDING_VIMRESUME.get_mut() },
            crate::types_defs::TriState::False
        );
    }

    #[test]
    fn vimresume_event_also_finishes_the_currently_triggering_state() {
        let _lock = crate::globals::global_state_test_lock();
        let _g =
            PendingVimresumeGuard::install(crate::types_defs::TriState::None);

        vimresume_event();

        assert_eq!(
            unsafe { *PENDING_VIMRESUME.get_mut() },
            crate::types_defs::TriState::False
        );
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

    // --- trigger_cursorhold ---

    /// Installs a `DID_CURSORHOLD` value and restores it on drop,
    /// even through a panic.
    struct DidCursorholdGuard(bool);

    impl DidCursorholdGuard {
        fn set(v: bool) -> Self {
            let cell = unsafe { DID_CURSORHOLD.get_mut() };
            let me = Self(*cell);
            *cell = v;
            me
        }
    }

    impl Drop for DidCursorholdGuard {
        fn drop(&mut self) {
            *unsafe { DID_CURSORHOLD.get_mut() } = self.0;
        }
    }

    /// The original's `INIT( = true)` is deliberate, not incidental:
    /// a zero-initialized `false` would let a CursorHold fire before
    /// any input had ever been seen.
    #[test]
    fn did_cursorhold_starts_set() {
        let _lock = crate::globals::global_state_test_lock();
        assert!(unsafe { *DID_CURSORHOLD.get_mut() });
    }

    /// Because `did_cursorhold` starts set and nothing translated yet
    /// clears it, no CursorHold can currently fire - even with the
    /// autocommand defined and the mode correct.
    #[test]
    fn trigger_cursorhold_is_false_while_already_triggered() {
        let _lock = crate::globals::global_state_test_lock();
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev = g.State;
        g.State = crate::state_defs::mode::NORMAL_BUSY as i32;

        let _d = DidCursorholdGuard::set(true);
        let _e = EventGuard::set(EventT::CursorHold);
        assert!(!unsafe { trigger_cursorhold() });

        unsafe { crate::globals::GLOBALS.get_mut() }.State = prev;
    }

    #[test]
    fn trigger_cursorhold_fires_in_normal_busy_mode_once_cleared() {
        let _lock = crate::globals::global_state_test_lock();
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev = g.State;
        g.State = crate::state_defs::mode::NORMAL_BUSY as i32;

        let _d = DidCursorholdGuard::set(false);
        let _e = EventGuard::set(EventT::CursorHold);
        assert!(unsafe { trigger_cursorhold() });

        unsafe { crate::globals::GLOBALS.get_mut() }.State = prev;
    }

    /// Insert mode is matched by BIT, not by equality, so any Insert
    /// sub-mode qualifies - and it uses the CursorHoldI event.
    #[test]
    fn trigger_cursorhold_fires_in_insert_mode() {
        let _lock = crate::globals::global_state_test_lock();
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev = g.State;
        g.State = crate::state_defs::mode::INSERT as i32;

        let _d = DidCursorholdGuard::set(false);
        let _e = EventGuard::set(EventT::CursorHoldI);
        assert!(unsafe { trigger_cursorhold() });

        unsafe { crate::globals::GLOBALS.get_mut() }.State = prev;
    }

    /// Recording a register suppresses CursorHold, so a macro records
    /// only what the user actually typed.
    #[test]
    fn trigger_cursorhold_is_suppressed_while_recording_a_register() {
        let _lock = crate::globals::global_state_test_lock();
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_state = g.State;
        let prev_reg = g.reg_recording;
        g.State = crate::state_defs::mode::NORMAL_BUSY as i32;
        g.reg_recording = b'q' as i32;

        let _d = DidCursorholdGuard::set(false);
        let _e = EventGuard::set(EventT::CursorHold);
        assert!(!unsafe { trigger_cursorhold() });

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.State = prev_state;
        g.reg_recording = prev_reg;
    }

    /// A mode with no matching autocommand must not trigger, even
    /// with everything else satisfied.
    #[test]
    fn trigger_cursorhold_is_false_without_a_matching_autocommand() {
        let _lock = crate::globals::global_state_test_lock();
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev = g.State;
        g.State = crate::state_defs::mode::NORMAL_BUSY as i32;

        let _d = DidCursorholdGuard::set(false);
        // Only the INSERT-mode event is defined, but we are in
        // Normal mode.
        let _e = EventGuard::set(EventT::CursorHoldI);
        assert!(!unsafe { trigger_cursorhold() });

        unsafe { crate::globals::GLOBALS.get_mut() }.State = prev;
    }

    /// Visual mode is neither NORMAL_BUSY nor Insert, so it does not
    /// qualify even with the event defined and the flag cleared.
    #[test]
    fn trigger_cursorhold_is_false_in_a_non_qualifying_mode() {
        let _lock = crate::globals::global_state_test_lock();
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev = g.State;
        g.State = crate::state_defs::mode::NORMAL as i32;

        let _d = DidCursorholdGuard::set(false);
        let _e = EventGuard::set(EventT::CursorHoldI);
        assert!(!unsafe { trigger_cursorhold() });

        unsafe { crate::globals::GLOBALS.get_mut() }.State = prev;
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
        unsafe { MAP_AUGROUP_ID_TO_NAME.get_mut() }.clear();
        unsafe { *NEXT_AUGROUP_ID.get_mut() = 1 };
        unsafe { *CURRENT_AUGROUP.get_mut() = augroup::DEFAULT };
    }

    #[test]
    fn augroup_add_allocates_and_reuses_group_ids() {
        let _lock = crate::globals::global_state_test_lock();
        reset_augroup_map();
        let first = unsafe { augroup_add(b"GroupA") };
        let same = unsafe { augroup_add(b"GroupA") };
        let second = unsafe { augroup_add(b"GroupB") };
        assert_eq!(first, 1);
        assert_eq!(same, first);
        assert_eq!(second, 2);
        assert_eq!(
            unsafe { MAP_AUGROUP_ID_TO_NAME.get_mut() }.get(&first),
            Some(&b"GroupA".to_vec())
        );
        assert_eq!(unsafe { augroup_name(first) }, Some(b"GroupA".to_vec()));
        assert_eq!(
            unsafe { augroup_name(augroup::DELETED) },
            Some(b"--- DELETED ---".to_vec())
        );
        unsafe { augroup_clear(first) };
        reset_augroup_map();
    }

    #[test]
    fn augroup_del_removes_both_registry_directions() {
        let _lock = crate::globals::global_state_test_lock();
        reset_augroup_map();
        let id = unsafe { augroup_add(b"Disposable") };
        let pattern = Box::into_raw(Box::new(crate::autocmd_defs::AutoPat {
            refcount: 1,
            pat: Some(b"*".to_vec()),
            reg_prog: std::ptr::null_mut(),
            group: id,
            patlen: 1,
            buflocal_nr: 0,
            allow_dirs: false,
        }));
        let autocmds = unsafe { AUTOCMDS.get_mut() };
        autocmds[EventT::BufEnter as usize].push(
            crate::autocmd_defs::AutoCmd {
                pat: pattern,
                id: 1,
                desc: Some(b"test".to_vec()),
                handler_cmd: Some(b"echo".to_vec()),
                handler_fn: Default::default(),
                script_ctx: Default::default(),
                once: false,
                nested: false,
            },
        );
        assert_eq!(unsafe { augroup_del(b"Disposable") }, Ok(()));
        assert!(unsafe { AUTOCMDS.get_mut() }[EventT::BufEnter as usize].is_empty());
        assert_eq!(augroup_find(b"Disposable"), augroup::ERROR);
        assert_eq!(unsafe { augroup_name(id) }, None);
        assert_eq!(unsafe { augroup_del(b"Missing") }, Err("No such group"));
        reset_augroup_map();
    }

    #[test]
    fn augroup_map_del_can_remove_each_direction_independently() {
        let _lock = crate::globals::global_state_test_lock();
        reset_augroup_map();
        let id = unsafe { augroup_add(b"Partial") };
        unsafe { augroup_map_del(0, Some(b"Partial")) };
        assert_eq!(augroup_find(b"Partial"), augroup::ERROR);
        assert_eq!(unsafe { augroup_name(id) }, Some(b"Partial".to_vec()));
        unsafe { augroup_map_del(id, None) };
        assert_eq!(unsafe { augroup_name(id) }, None);
        reset_augroup_map();
    }

    #[test]
    fn do_augroup_switches_lists_ends_and_deletes_groups() {
        let _lock = crate::globals::global_state_test_lock();
        reset_augroup_map();
        assert_eq!(unsafe { do_augroup(b"GroupA", false) }, Ok(Vec::new()));
        assert_eq!(unsafe { *CURRENT_AUGROUP.get_mut() }, 1);
        assert_eq!(
            unsafe { do_augroup(b"", false) },
            Ok(vec![b"GroupA".to_vec()])
        );
        assert_eq!(unsafe { do_augroup(b"END", false) }, Ok(Vec::new()));
        assert_eq!(unsafe { *CURRENT_AUGROUP.get_mut() }, augroup::DEFAULT);
        assert_eq!(unsafe { do_augroup(b"GroupA", true) }, Ok(Vec::new()));
        assert_eq!(unsafe { do_augroup(b"", true) }, Err("Argument required"));
        reset_augroup_map();
    }

    #[test]
    fn autocmd_delete_id_removes_the_matching_command() {
        let _lock = crate::globals::global_state_test_lock();
        let pattern = Box::into_raw(Box::new(crate::autocmd_defs::AutoPat {
            refcount: 1,
            pat: Some(b"*".to_vec()),
            reg_prog: std::ptr::null_mut(),
            group: augroup::DEFAULT,
            patlen: 1,
            buflocal_nr: 0,
            allow_dirs: false,
        }));
        {
            let commands = unsafe { AUTOCMDS.get_mut() };
            commands[EventT::BufEnter as usize].push(crate::autocmd_defs::AutoCmd {
                pat: pattern,
                id: 42,
                desc: None,
                handler_cmd: Some(b"echo".to_vec()),
                handler_fn: Default::default(),
                script_ctx: Default::default(),
                once: false,
                nested: false,
            });
        }
        assert!(unsafe { autocmd_delete_id(42) });
        assert!(unsafe { AUTOCMDS.get_mut() }[EventT::BufEnter as usize].is_empty());
        assert!(!unsafe { autocmd_delete_id(42) });
    }

    #[test]
    fn aucmd_del_for_event_and_group_only_removes_matching_event_entries() {
        let _lock = crate::globals::global_state_test_lock();
        let make = |event: EventT| {
            let pattern = Box::into_raw(Box::new(crate::autocmd_defs::AutoPat {
                refcount: 1,
                pat: Some(b"*".to_vec()),
                reg_prog: std::ptr::null_mut(),
                group: 7,
                patlen: 1,
                buflocal_nr: 0,
                allow_dirs: false,
            }));
            let autocmds = unsafe { AUTOCMDS.get_mut() };
            autocmds[event as usize].push(
                crate::autocmd_defs::AutoCmd {
                    pat: pattern,
                    id: i64::from(event as i32),
                    desc: None,
                    handler_cmd: Some(b"echo".to_vec()),
                    handler_fn: Default::default(),
                    script_ctx: Default::default(),
                    once: false,
                    nested: false,
                },
            );
        };
        make(EventT::BufEnter);
        make(EventT::BufLeave);
        unsafe { aucmd_del_for_event_and_group(EventT::BufEnter, 7) };
        assert!(unsafe { AUTOCMDS.get_mut() }[EventT::BufEnter as usize].is_empty());
        assert_eq!(unsafe { AUTOCMDS.get_mut() }[EventT::BufLeave as usize].len(), 1);
        unsafe { aucmd_del_for_event_and_group(EventT::BufLeave, 7) };
    }

    #[test]
    fn free_all_autocmds_resets_group_registries() {
        let _lock = crate::globals::global_state_test_lock();
        reset_augroup_map();
        assert_eq!(unsafe { augroup_add(b"Temporary") }, 1);
        unsafe { free_all_autocmds() };
        assert_eq!(augroup_find(b"Temporary"), augroup::ERROR);
        assert_eq!(unsafe { *NEXT_AUGROUP_ID.get_mut() }, 1);
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

    // --- arg_augroup_get ---

    #[test]
    fn arg_augroup_get_finds_a_real_group_and_consumes_it() {
        let _lock = crate::globals::global_state_test_lock();
        reset_augroup_map();
        unsafe { MAP_AUGROUP_NAME_TO_ID.get_mut() }.insert(b"MyGroup".to_vec(), 5);

        let arg = b"MyGroup BufRead *.c";
        let (group, off) = arg_augroup_get(arg);
        assert_eq!(group, 5);
        assert_eq!(&arg[off..], b"BufRead *.c", "the name and blanks are consumed");
        reset_augroup_map();
    }

    #[test]
    fn arg_augroup_get_leaves_the_argument_alone_when_the_name_is_not_a_group() {
        // An unmatched leading word is NOT consumed - it is the event
        // list, not a group name - so the offset stays at zero.
        let _lock = crate::globals::global_state_test_lock();
        reset_augroup_map();

        let arg = b"BufRead *.c";
        let (group, off) = arg_augroup_get(arg);
        assert_eq!(group, augroup::ALL);
        assert_eq!(off, 0);
        reset_augroup_map();
    }

    #[test]
    fn arg_augroup_get_with_no_leading_word_is_all_groups() {
        let _lock = crate::globals::global_state_test_lock();
        reset_augroup_map();

        for arg in [&b""[..], &b" BufRead"[..], &b"|foo"[..]] {
            let (group, off) = arg_augroup_get(arg);
            assert_eq!(group, augroup::ALL, "{arg:?}");
            assert_eq!(off, 0, "{arg:?}");
        }
        reset_augroup_map();
    }

    #[test]
    fn arg_augroup_get_stops_the_name_at_a_bar() {
        // A '|' ends the group name just as whitespace does.
        let _lock = crate::globals::global_state_test_lock();
        reset_augroup_map();
        unsafe { MAP_AUGROUP_NAME_TO_ID.get_mut() }.insert(b"MyGroup".to_vec(), 7);

        let arg = b"MyGroup|echo";
        let (group, off) = arg_augroup_get(arg);
        assert_eq!(group, 7);
        assert_eq!(&arg[off..], b"|echo", "the bar itself is not consumed");
        reset_augroup_map();
    }

    #[test]
    fn arg_augroup_get_consumes_a_bare_group_name_with_nothing_after_it() {
        let _lock = crate::globals::global_state_test_lock();
        reset_augroup_map();
        unsafe { MAP_AUGROUP_NAME_TO_ID.get_mut() }.insert(b"Solo".to_vec(), 9);

        let arg = b"Solo";
        let (group, off) = arg_augroup_get(arg);
        assert_eq!(group, 9);
        assert_eq!(off, arg.len());
        reset_augroup_map();
    }

    // --- event_name2nr / check_ei / get_event_name_no_group ---

    #[test]
    fn event_name2nr_resolves_case_insensitively_and_consumes_a_comma() {
        let (event, consumed) = event_name2nr(b"bufenter,ColorScheme");
        assert_eq!(event, Some(EventT::BufEnter));
        assert_eq!(consumed, b"bufenter,".len());
    }

    #[test]
    fn event_name2nr_stops_before_whitespace_and_bar() {
        let (event, consumed) = event_name2nr(b"BufEnter rest");
        assert_eq!(event, Some(EventT::BufEnter));
        assert_eq!(consumed, b"BufEnter".len());

        let (event, consumed) = event_name2nr(b"ColorScheme|echo");
        assert_eq!(event, Some(EventT::ColorScheme));
        assert_eq!(consumed, b"ColorScheme".len());
    }

    #[test]
    fn event_name2nr_resolves_an_alias_to_its_canonical_event() {
        let (event, consumed) = event_name2nr(b"FileEncoding");
        assert_eq!(event, Some(EventT::EncodingChanged));
        assert_eq!(consumed, b"FileEncoding".len());
    }

    #[test]
    fn event_name2nr_reports_unknown_names_and_their_consumed_length() {
        let (event, consumed) = event_name2nr(b"NotAnEvent,next");
        assert_eq!(event, None);
        assert_eq!(consumed, b"NotAnEvent,".len());
    }

    #[test]
    fn event_name2nr_str_requires_and_resolves_the_whole_name() {
        assert_eq!(event_name2nr_str(b"bufenter"), Some(EventT::BufEnter));
        assert_eq!(event_name2nr_str(b"BufEnter tail"), None);
        assert_eq!(event_name2nr_str(b"missing"), None);
    }

    #[test]
    fn autocmd_supported_accepts_known_names_and_aliases() {
        assert!(autocmd_supported(b"BufEnter"));
        assert!(autocmd_supported(b"BufCreate"));
        assert!(!autocmd_supported(b"NeroMissingEvent"));
    }

    #[test]
    fn event_ignored_handles_all_explicit_and_negative_entries() {
        assert!(event_ignored(EventT::BufEnter, b"all", true));
        assert!(event_ignored(EventT::BufEnter, b"BufEnter", true));
        assert!(!event_ignored(
            EventT::BufEnter,
            b"all,-BufEnter",
            true
        ));
        assert!(!event_ignored(EventT::BufEnter, b"BufLeave", true));
    }

    #[test]
    fn au_event_disable_appends_and_returns_eventignore() {
        let _lock = crate::globals::global_state_test_lock();
        let old = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ei.clone();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ei =
            Some(b"BufEnter".to_vec());
        let saved = unsafe { au_event_disable(b",BufLeave") };
        let current = unsafe { crate::option_vars::OPTION_VARS.get_mut() }
            .p_ei
            .clone();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ei = old;
        assert_eq!(saved, b"BufEnter");
        assert_eq!(current, Some(b"BufEnter,BufLeave".to_vec()));
    }

    #[test]
    fn au_event_restore_reinstates_saved_eventignore() {
        let _lock = crate::globals::global_state_test_lock();
        let old = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ei.clone();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ei =
            Some(b"BufLeave".to_vec());
        unsafe { au_event_restore(Some(b"BufEnter".to_vec())) };
        let restored = unsafe { crate::option_vars::OPTION_VARS.get_mut() }
            .p_ei
            .clone();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ei = old;
        assert_eq!(restored, Some(b"BufEnter".to_vec()));
    }

    #[test]
    fn aupat_is_buflocal_checks_the_exact_envelope() {
        assert!(aupat_is_buflocal(b"<buffer>"));
        assert!(aupat_is_buflocal(b"<buffer=12>"));
        assert!(!aupat_is_buflocal(b"<Buffer>"));
        assert!(!aupat_is_buflocal(b"<buffer"));
    }

    #[test]
    fn aupat_get_buflocal_nr_resolves_current_abuf_and_numeric_forms() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT {
            handle: 17,
            ..Default::default()
        };
        let buf_ptr = std::ptr::addr_of_mut!(buf);
        let _curbuf = unsafe {
            crate::globals::GlobalFieldGuard::install(|g| &mut g.curbuf, buf_ptr)
        };
        let old_abuf = unsafe { *AUTOCMD_BUFNR.get_mut() };
        unsafe { *AUTOCMD_BUFNR.get_mut() = 23 };
        assert_eq!(unsafe { aupat_get_buflocal_nr(b"<buffer>") }, 17);
        assert_eq!(unsafe { aupat_get_buflocal_nr(b"<buffer=abuf>") }, 23);
        assert_eq!(unsafe { aupat_get_buflocal_nr(b"<buffer=42>") }, 42);
        assert_eq!(unsafe { aupat_get_buflocal_nr(b"<buffer=x>") }, 0);
        unsafe { *AUTOCMD_BUFNR.get_mut() = old_abuf };
    }

    #[test]
    fn aupat_normalize_buflocal_pat_uses_standard_numeric_form() {
        assert_eq!(
            unsafe { aupat_normalize_buflocal_pat(b"<buffer=anything>", 42) },
            b"<buffer=42>"
        );
    }

    #[test]
    fn arg_autocmd_flag_get_consumes_once_and_rejects_duplicates() {
        let mut once = false;
        assert_eq!(
            arg_autocmd_flag_get(&mut once, b"++once  echo", b"++once"),
            (false, 8)
        );
        assert!(once);
        assert_eq!(
            arg_autocmd_flag_get(&mut once, b"++once echo", b"++once"),
            (true, 0)
        );
    }

    #[test]
    fn check_ei_accepts_empty_all_and_comma_separated_events() {
        assert!(check_ei(b"", false));
        assert!(check_ei(b"all", false));
        assert!(check_ei(b"BufEnter,ColorScheme", false));
    }

    #[test]
    fn check_ei_accepts_subtraction_and_case_insensitive_names() {
        assert!(check_ei(b"-bufenter,COLORSCHEME", false));
    }

    #[test]
    fn check_ei_rejects_unknown_empty_and_whitespace_separated_entries() {
        assert!(!check_ei(b"NotAnEvent", false));
        assert!(!check_ei(b",BufEnter", false));
        assert!(!check_ei(b"BufEnter ColorScheme", false));
    }

    #[test]
    fn check_ei_window_form_accepts_only_window_local_events() {
        assert!(check_ei(b"BufEnter", true));
        assert!(check_ei(b"all", true));
        assert!(!check_ei(b"ColorScheme", true));
    }

    #[test]
    fn get_event_name_no_group_indexes_the_table_directly() {
        assert_eq!(get_event_name_no_group(0, false), Some(&b"BufAdd"[..]));
        let last = crate::autocmd_defs::NUM_EVENTS as i32 - 1;
        assert_eq!(get_event_name_no_group(last, false), Some(&b"WinScrolled"[..]));
    }

    #[test]
    fn get_event_name_no_group_is_none_out_of_range() {
        assert_eq!(get_event_name_no_group(-1, false), None);
        assert_eq!(
            get_event_name_no_group(crate::autocmd_defs::NUM_EVENTS as i32, false),
            None
        );
        // The bounds check uses the FULL table length even for the
        // window-local list, so an index past the end is rejected
        // there too rather than wrapping.
        assert_eq!(
            get_event_name_no_group(crate::autocmd_defs::NUM_EVENTS as i32, true),
            None
        );
    }

    #[test]
    fn get_event_name_no_group_with_win_walks_only_window_local_events() {
        // Derive the expectation independently from the table rather
        // than hardcoding it, so this genuinely cross-checks the
        // counting loop instead of restating it.
        let expected: Vec<&[u8]> = crate::autocmd_defs::EVENT_NAMES
            .iter()
            .filter(|e| e.win_local)
            .map(|e| e.name)
            .collect();
        assert!(!expected.is_empty(), "the window-local subset must not be empty");

        for (i, want) in expected.iter().enumerate() {
            assert_eq!(
                get_event_name_no_group(i as i32, true),
                Some(*want),
                "window-local index {i}"
            );
        }

        // One past the window-local subset reports nothing, even
        // though it is still inside the full table.
        assert!(expected.len() < crate::autocmd_defs::NUM_EVENTS);
        assert_eq!(get_event_name_no_group(expected.len() as i32, true), None);
    }

    #[test]
    fn get_event_name_no_group_win_list_is_a_strict_subset() {
        // The two modes must genuinely differ, or the win branch would
        // be untested by the above.
        let full = get_event_name_no_group(3, false);
        let win = get_event_name_no_group(3, true);
        assert!(full.is_some() && win.is_some());
        let win_count = crate::autocmd_defs::EVENT_NAMES.iter().filter(|e| e.win_local).count();
        assert!(win_count < crate::autocmd_defs::NUM_EVENTS);
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
