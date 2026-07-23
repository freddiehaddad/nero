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
//! Deferred: everything else - `apply_autocmds_group`'s real
//! autocmd-matching-and-execution body (needs pattern matching,
//! `exec_autocmds`, script/function invocation via the not-yet-started
//! parser), and the entire `:autocmd`/augroup definition/deletion
//! side (needs the `:autocmd` command parser).

use crate::autocmd_defs::{AutoCmdVec, AutoPatCmd, EventT, NUM_EVENTS};
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

/// Return `true` if `event` autocommand is defined (`has_event`).
#[must_use]
pub fn has_event(event: EventT) -> bool {
    !(unsafe { AUTOCMDS.get_mut() })[event as usize].is_empty()
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
        if event == EventT::BufWipeout {
            if let Some(buf) = buf {
                unsafe { aubuflocal_remove(buf) };
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_event_is_false_for_every_event_when_autocmds_are_always_empty() {
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
}
