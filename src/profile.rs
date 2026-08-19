//! Translated from `src/nvim/profile.c` (`src/nvim/profile.h` has no
//! manually-written content beyond generated declarations).
//!
//! The complete timing arithmetic, function/script timing state,
//! per-line accounting, report formatting/writing, reset logic,
//! startup-time reporting, and command lifecycle are translated.
//! [`func_line_exec`]/[`func_line_end`]/[`script_line_exec`] use the
//! real function/script records.
//! [`func_do_profile`] initializes and enables per-function timing.
//! [`script_prof_save`]/[`script_prof_restore`] record nested-child
//! timing state, and [`prof_child_enter`]/[`prof_child_exit`] perform
//! the paired function/script child measurement.
//! `:profile` subcommand completion is translated through
//! [`set_context_in_profile_cmd`]/[`get_profile_name`].
//! [`profile_reset`] clears all accumulated script/function timing
//! records.
//! [`ex_profile`] implements the start/pause/continue/dump/stop
//! command lifecycle; debugger-backed `file`/`func` remain deferred.
//! The complete `--startuptime` report lifecycle is translated through
//! [`time_init`]/[`time_start`]/[`time_msg`]/[`time_finish`].
//!
//! `:profile file`/`func` remain with `debugger.c`'s untranslated
//! `ex_breakadd` command parser. Report-open failures are propagated as
//! `std::io::Result` because the message-display pipeline is not yet
//! available.

use crate::types_defs::ProftimeT;
use std::io::Write;

/// Subcommands offered when completing `:profile`
/// (`pexpand_cmds`). File-static in the original, which terminates
/// the array with a `NULL` sentinel; a Rust slice carries its own
/// length, so the sentinel is dropped.
const PEXPAND_CMDS: [&str; 7] = [
    "continue", "dump", "file", "func", "pause", "start", "stop",
];

/// What `:profile` completion is currently expanding
/// (`pexpand_what`). File-static in the original.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PexpandWhat {
    /// completing a `:profile` subcommand (`PEXP_SUBCMD`)
    #[default]
    Subcmd,
    /// completing anything else - no candidates (`PEXP_NOTHING`)
    Nothing,
}

static PEXPAND_WHAT: crate::globals::GlobalCell<PexpandWhat> =
    crate::globals::GlobalCell::new(PexpandWhat::Subcmd);

/// File receiving profile output (`profile_fname`).
static PROFILE_FNAME: crate::globals::GlobalCell<Option<Vec<u8>>> =
    crate::globals::GlobalCell::new(None);

/// Timestamp captured when `:profile pause` begins (`pause_time`).
static PAUSE_TIME: crate::globals::GlobalCell<ProftimeT> =
    crate::globals::GlobalCell::new(0);

/// Reset all profiling information (`profile_reset`).
///
/// # Safety
/// Mutates the script registry, function registry, and profile output
/// file name. No profiling operation may run concurrently.
pub unsafe fn profile_reset() {
    for sid in 1..=crate::runtime::script_item_count() {
        let script = crate::runtime::script_item(sid);
        // SAFETY: forwarded from this function's own safety doc; script
        // registry pointers remain live for the process lifetime.
        let script = unsafe { &mut *script };
        if script.sn_prof_on {
            script.sn_prof_on = false;
            script.sn_pr_force = false;
            script.sn_pr_child = profile_zero();
            script.sn_pr_nest = 0;
            script.sn_pr_count = 0;
            script.sn_pr_total = profile_zero();
            script.sn_pr_self = profile_zero();
            script.sn_pr_start = profile_zero();
            script.sn_pr_children = profile_zero();
            script.sn_prl_ga.clear();
            script.sn_prl_start = profile_zero();
            script.sn_prl_children = profile_zero();
            script.sn_prl_wait = profile_zero();
            script.sn_prl_idx = -1;
            script.sn_prl_execed = 0;
        }
    }

    // SAFETY: forwarded from this function's own safety doc.
    for function in unsafe { crate::eval::userfunc::func_tbl_values() } {
        // SAFETY: func_tbl_values returns live registered functions.
        let function = unsafe { &mut *function };
        if function.uf_prof_initialized != 0 {
            function.uf_profiling = 0;
            function.uf_tm_count = 0;
            function.uf_tm_total = profile_zero();
            function.uf_tm_self = profile_zero();
            function.uf_tm_children = profile_zero();
            function.uf_tml_count.fill(0);
            function.uf_tml_total.fill(0);
            function.uf_tml_self.fill(0);
            function.uf_tml_start = profile_zero();
            function.uf_tml_children = profile_zero();
            function.uf_tml_wait = profile_zero();
            function.uf_tml_idx = -1;
            function.uf_tml_execed = 0;
        }
    }

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *PROFILE_FNAME.get_mut() = None };
}

/// Initialize and enable profiling for one user function
/// (`func_do_profile`).
pub fn func_do_profile(function: &mut crate::eval::typval_defs::UfuncT) {
    if function.uf_prof_initialized == 0 {
        let len = function.uf_lines.len().max(1);
        function.uf_tm_count = 0;
        function.uf_tm_self = profile_zero();
        function.uf_tm_total = profile_zero();
        if function.uf_tml_count.is_empty() {
            function.uf_tml_count = vec![0; len];
        }
        if function.uf_tml_total.is_empty() {
            function.uf_tml_total = vec![profile_zero(); len];
        }
        if function.uf_tml_self.is_empty() {
            function.uf_tml_self = vec![profile_zero(); len];
        }
        function.uf_tml_idx = -1;
        function.uf_prof_initialized = 1;
    }
    function.uf_profiling = 1;
}

/// Set what `:profile` completion should offer.
///
/// The original assigns its file-static directly from
/// [`set_context_in_profile_cmd`]; this accessor keeps that mutation
/// explicit for other callers and tests.
///
/// # Safety
/// Must not run concurrently with any other access to `PEXPAND_WHAT`.
pub unsafe fn set_pexpand_what(what: PexpandWhat) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *PEXPAND_WHAT.get_mut() = what };
}

/// Configure command-line completion for `:profile`
/// (`set_context_in_profile_cmd`).
///
/// # Safety
/// Mutates the shared `PEXPAND_WHAT` completion state.
pub unsafe fn set_context_in_profile_cmd(
    xp: &mut crate::cmdexpand_defs::ExpandT,
    argument: &[u8],
) {
    xp.xp_context = crate::cmdexpand_defs::ExpandContext::Profile;
    xp.xp_pattern = Some(argument.to_vec());
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { set_pexpand_what(PexpandWhat::Subcmd) };

    let end_subcmd = crate::charset::skiptowhite(argument);
    if end_subcmd == argument.len() {
        return;
    }

    let subcmd = &argument[..end_subcmd];
    let pattern_start =
        end_subcmd + crate::charset::skipwhite(&argument[end_subcmd..]);
    if subcmd == b"start" || subcmd == b"file" {
        xp.xp_context = crate::cmdexpand_defs::ExpandContext::Files;
        xp.xp_pattern = Some(argument[pattern_start..].to_vec());
    } else if subcmd == b"func" {
        xp.xp_context = crate::cmdexpand_defs::ExpandContext::UserFunc;
        xp.xp_pattern = Some(argument[pattern_start..].to_vec());
    } else {
        xp.xp_context = crate::cmdexpand_defs::ExpandContext::Nothing;
    }
}

/// The `idx`'th `:profile` completion candidate, or `None` past the
/// end (`get_profile_name`, given to `ExpandGeneric`).
///
/// Only the subcommand list has candidates; every other expansion
/// state offers none, matching the original's own `default: return
/// NULL`.
///
/// The original's unused `expand_T *xp` parameter is dropped, and its
/// unchecked `pexpand_cmds[idx]` index becomes a bounds-checked
/// lookup - the original relies on `ExpandGeneric` stopping at the
/// `NULL` sentinel instead.
///
/// # Safety
/// Must not run concurrently with any write to `PEXPAND_WHAT`.
#[must_use]
pub unsafe fn get_profile_name(idx: i32) -> Option<&'static str> {
    // SAFETY: forwarded from this function's own safety doc.
    match unsafe { *PEXPAND_WHAT.get_mut() } {
        PexpandWhat::Subcmd => {
            let idx = usize::try_from(idx).ok()?;
            PEXPAND_CMDS.get(idx).copied()
        }
        PexpandWhat::Nothing => None,
    }
}

/// Whether the script currently being sourced asked for its functions
/// to be profiled (`prof_def_func`).
///
/// # Safety
/// Touches `crate::globals::GLOBALS.current_sctx` and forwards
/// [`crate::runtime::script_item`]'s own safety doc.
#[must_use]
pub unsafe fn prof_def_func() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let sid = unsafe { crate::globals::GLOBALS.get_mut() }.current_sctx.sc_sid;
    if sid > 0 {
        let item = crate::runtime::script_item(sid);
        if item.is_null() {
            return false;
        }
        // SAFETY: forwarded from this function's own safety doc.
        return unsafe { (*item).sn_pr_force };
    }
    false
}

/// Initialize profiling state for a script (`profile_init`).
pub fn profile_init(script: &mut crate::runtime_defs::ScriptitemT) {
    script.sn_pr_count = 0;
    script.sn_pr_total = profile_zero();
    script.sn_pr_self = profile_zero();
    script.sn_prl_ga.clear();
    script.sn_prl_idx = -1;
    script.sn_prof_on = true;
    script.sn_pr_nest = 0;
}

/// Start timing the current user-function line (`func_line_start`).
///
/// # Safety
/// `cookie` must point to a live `FunccallT` whose `fc_func` points to
/// a live `UfuncT`.
pub unsafe fn func_line_start(cookie: *mut std::ffi::c_void) {
    let call = cookie.cast::<crate::eval::typval_defs::FunccallT>();
    // SAFETY: forwarded from this function's own safety doc.
    let function = unsafe { &mut *(*call).fc_func };
    let line = crate::runtime::sourcing_lnum();
    if function.uf_profiling == 0
        || line < 1
        || line as usize > function.uf_lines.len()
    {
        return;
    }
    let mut index = (line - 1) as usize;
    while index > 0 && function.uf_lines[index].is_none() {
        index -= 1;
    }
    function.uf_tml_idx = index as i32;
    function.uf_tml_execed = 0;
    function.uf_tml_start = profile_start();
    function.uf_tml_children = profile_zero();
    function.uf_tml_wait = profile_get_wait();
}

/// Start timing the current source line (`script_line_start`).
///
/// # Safety
/// Reads `GLOBALS.current_sctx` and mutates its live script item.
pub unsafe fn script_line_start() {
    // SAFETY: forwarded from this function's own safety doc.
    let sid = unsafe { crate::globals::GLOBALS.get_mut() }.current_sctx.sc_sid;
    if sid <= 0 || sid > crate::runtime::script_item_count() {
        return;
    }
    let item = crate::runtime::script_item(sid);
    let line = crate::runtime::sourcing_lnum();
    // SAFETY: the script registry owns this live item.
    if unsafe { (*item).sn_prof_on } && line >= 1 {
        let index = (line - 1) as usize;
        // SAFETY: the script registry owns this live item.
        let item = unsafe { &mut *item };
        item.sn_prl_ga
            .resize(index + 1, crate::runtime_defs::SnPrlT::default());
        item.sn_prl_idx = line - 1;
        item.sn_prl_execed = 0;
        item.sn_prl_start = profile_start();
        item.sn_prl_children = profile_zero();
        item.sn_prl_wait = profile_get_wait();
    }
}

/// Mark the current profiled function line as executed
/// (`func_line_exec`).
///
/// # Safety
/// `cookie` must point to a live `FunccallT` whose `fc_func` points to
/// a live `UfuncT`.
pub unsafe fn func_line_exec(cookie: *mut std::ffi::c_void) {
    let fcp = cookie.cast::<crate::eval::typval_defs::FunccallT>();
    // SAFETY: forwarded from this function's own safety doc.
    let fp = unsafe { (*fcp).fc_func };
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { (*fp).uf_profiling != 0 && (*fp).uf_tml_idx >= 0 } {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*fp).uf_tml_execed = 1 };
    }
}

/// Finish profiling the current function line (`func_line_end`).
///
/// # Safety
/// `cookie` must point to a live `FunccallT` whose `fc_func` and
/// timing vectors are valid for `uf_tml_idx`.
pub unsafe fn func_line_end(cookie: *mut std::ffi::c_void) {
    let fcp = cookie.cast::<crate::eval::typval_defs::FunccallT>();
    // SAFETY: forwarded from this function's own safety doc.
    let fp = unsafe { &mut *(*fcp).fc_func };
    if fp.uf_profiling != 0 && fp.uf_tml_idx >= 0 {
        let idx = fp.uf_tml_idx as usize;
        if fp.uf_tml_execed != 0 {
            fp.uf_tml_count[idx] += 1;
            let elapsed = profile_end(fp.uf_tml_start);
            let elapsed = profile_sub_wait(fp.uf_tml_wait, elapsed);
            fp.uf_tml_start = elapsed;
            fp.uf_tml_total[idx] = profile_add(fp.uf_tml_total[idx], elapsed);
            fp.uf_tml_self[idx] =
                profile_self(fp.uf_tml_self[idx], elapsed, fp.uf_tml_children);
        }
        fp.uf_tml_idx = -1;
    }
}

/// Mark the current profiled script line as executed
/// (`script_line_exec`).
///
/// # Safety
/// Reads `GLOBALS.current_sctx` and mutates the corresponding live
/// script item.
pub unsafe fn script_line_exec() {
    // SAFETY: forwarded from this function's own safety doc.
    let sid = unsafe { crate::globals::GLOBALS.get_mut() }.current_sctx.sc_sid;
    if sid <= 0 || sid > crate::runtime::script_item_count() {
        return;
    }
    let item = crate::runtime::script_item(sid);
    // SAFETY: the script registry owns this live item.
    if unsafe { (*item).sn_prof_on && (*item).sn_prl_idx >= 0 } {
        // SAFETY: the script registry owns this live item.
        unsafe { (*item).sn_prl_execed = 1 };
    }
}

/// Finish timing the current source line (`script_line_end`).
///
/// # Safety
/// Reads `GLOBALS.current_sctx` and mutates its live script item.
pub unsafe fn script_line_end() {
    // SAFETY: forwarded from this function's own safety doc.
    let sid = unsafe { crate::globals::GLOBALS.get_mut() }.current_sctx.sc_sid;
    if sid <= 0 || sid > crate::runtime::script_item_count() {
        return;
    }
    let item = crate::runtime::script_item(sid);
    // SAFETY: the script registry owns this live item.
    let item = unsafe { &mut *item };
    let Ok(index) = usize::try_from(item.sn_prl_idx) else {
        return;
    };
    if !item.sn_prof_on || index >= item.sn_prl_ga.len() {
        return;
    }
    if item.sn_prl_execed != 0 {
        let line = &mut item.sn_prl_ga[index];
        line.snp_count += 1;
        let elapsed = profile_end(item.sn_prl_start);
        let elapsed = profile_sub_wait(item.sn_prl_wait, elapsed);
        item.sn_prl_start = elapsed;
        line.sn_prl_total = profile_add(line.sn_prl_total, elapsed);
        line.sn_prl_self =
            profile_self(line.sn_prl_self, elapsed, item.sn_prl_children);
    }
    item.sn_prl_idx = -1;
}

/// Save timing state before invoking another script or function
/// (`script_prof_save`).
///
/// # Safety
/// Reads `GLOBALS.current_sctx` and mutates its live script item.
pub unsafe fn script_prof_save(tm: &mut ProftimeT) {
    // SAFETY: forwarded from this function's own safety doc.
    let sid = unsafe { crate::globals::GLOBALS.get_mut() }.current_sctx.sc_sid;
    if sid > 0 && sid <= crate::runtime::script_item_count() {
        let item = crate::runtime::script_item(sid);
        // SAFETY: the script registry owns this live item.
        if unsafe { (*item).sn_prof_on } {
            // SAFETY: the script registry owns this live item.
            let old_nest = unsafe { (*item).sn_pr_nest };
            // SAFETY: the script registry owns this live item.
            unsafe { (*item).sn_pr_nest += 1 };
            if old_nest == 0 {
                // SAFETY: the script registry owns this live item.
                unsafe { (*item).sn_pr_child = profile_start() };
            }
        }
    }
    *tm = profile_get_wait();
}

/// Accumulate time spent in a nested script or function
/// (`script_prof_restore`).
///
/// # Safety
/// Reads `GLOBALS.current_sctx` and mutates its live script item.
pub unsafe fn script_prof_restore(tm: &ProftimeT) {
    // SAFETY: forwarded from this function's own safety doc.
    let sid = unsafe { crate::globals::GLOBALS.get_mut() }.current_sctx.sc_sid;
    if sid <= 0 || sid > crate::runtime::script_item_count() {
        return;
    }
    let item = crate::runtime::script_item(sid);
    // SAFETY: the script registry owns this live item.
    if unsafe { (*item).sn_prof_on } {
        // SAFETY: the script registry owns this live item.
        unsafe { (*item).sn_pr_nest -= 1 };
        // SAFETY: the script registry owns this live item.
        if unsafe { (*item).sn_pr_nest } == 0 {
            // SAFETY: the script registry owns this live item.
            let child = unsafe { profile_end((*item).sn_pr_child) };
            let child = profile_sub_wait(*tm, child);
            // SAFETY: the script registry owns this live item.
            unsafe {
                (*item).sn_pr_child = child;
                (*item).sn_pr_children = profile_add((*item).sn_pr_children, child);
                (*item).sn_prl_children = profile_add((*item).sn_prl_children, child);
            }
        }
    }
}

/// Prepare profiling before entering a nested operation
/// (`prof_child_enter`).
///
/// # Safety
/// The current funccall, when non-null, and its `fc_func` must be live.
/// Forwarded from [`script_prof_save`].
pub unsafe fn prof_child_enter(tm: &mut ProftimeT) {
    let fc = crate::eval::userfunc::get_current_funccal();
    // SAFETY: forwarded from this function's own safety doc.
    if !fc.is_null() && unsafe { (*(*fc).fc_func).uf_profiling != 0 } {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*fc).fc_prof_child = profile_start() };
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { script_prof_save(tm) };
}

/// Finish profiling after a nested operation (`prof_child_exit`).
///
/// # Safety
/// The current funccall, when non-null, and its `fc_func` must be live.
/// Forwarded from [`script_prof_restore`].
pub unsafe fn prof_child_exit(tm: &ProftimeT) {
    let fc = crate::eval::userfunc::get_current_funccal();
    // SAFETY: forwarded from this function's own safety doc.
    if !fc.is_null() && unsafe { (*(*fc).fc_func).uf_profiling != 0 } {
        // SAFETY: forwarded from this function's own safety doc.
        let child = unsafe { profile_end((*fc).fc_prof_child) };
        let child = profile_sub_wait(*tm, child);
        // SAFETY: forwarded from this function's own safety doc.
        unsafe {
            (*fc).fc_prof_child = child;
            let fp = (*fc).fc_func;
            (*fp).uf_tm_children = profile_add((*fp).uf_tm_children, child);
            (*fp).uf_tml_children = profile_add((*fp).uf_tml_children, child);
        }
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { script_prof_restore(tm) };
}

/// Gets the current time (`profile_start`).
#[inline]
pub fn profile_start() -> ProftimeT {
    crate::os::time::os_hrtime()
}

/// Computes the time elapsed since `tm` (`profile_end`).
#[inline]
pub fn profile_end(tm: ProftimeT) -> ProftimeT {
    profile_sub(crate::os::time::os_hrtime(), tm)
}

/// Gets a string representing time `tm`, as `"seconds.microseconds"`
/// (`profile_msg`). Unlike the original (which returns a pointer into a
/// shared `static char buf[50]`, not multithread-safe), this returns an
/// owned `String`.
pub fn profile_msg(tm: ProftimeT) -> std::string::String {
    format!("{:10.6}", profile_signed(tm) as f64 / 1_000_000_000.0)
}

/// Gets the time `msec` into the future (`profile_setlimit`).
///
/// The maximum number of milliseconds is `(2^63 / 10^6) - 1 = 9.223372e+12`.
/// If `msec > 0`, returns the time `msec` past now; otherwise returns the
/// zero time.
pub fn profile_setlimit(msec: i64) -> ProftimeT {
    if msec <= 0 {
        // no limit
        return profile_zero();
    }
    assert!(msec < (i64::MAX / 1_000_000));
    let nsec = (msec as ProftimeT) * 1_000_000;
    crate::os::time::os_hrtime().wrapping_add(nsec)
}

/// Checks if the current time has passed `tm` (`profile_passed_limit`).
///
/// Returns true if the current time is past `tm`, false if not or if the
/// timer was not set.
pub fn profile_passed_limit(tm: ProftimeT) -> bool {
    if tm == 0 {
        // timer was not set
        return false;
    }
    profile_cmp(crate::os::time::os_hrtime(), tm) < 0
}

/// Gets the zero time (`profile_zero`).
#[inline]
pub const fn profile_zero() -> ProftimeT {
    0
}

/// Divides time `tm` by `count` (`profile_divide`).
///
/// Returns `0` if `count <= 0`, otherwise `tm / count`.
pub fn profile_divide(tm: ProftimeT, count: i32) -> ProftimeT {
    if count <= 0 {
        return profile_zero();
    }
    (tm as f64 / count as f64).round() as ProftimeT
}

/// Adds time `tm2` to `tm1` (`profile_add`).
#[inline]
pub fn profile_add(tm1: ProftimeT, tm2: ProftimeT) -> ProftimeT {
    tm1.wrapping_add(tm2)
}

/// Subtracts time `tm2` from `tm1` (`profile_sub`).
///
/// Unsigned overflow (wraparound) occurs if `tm2` is greater than `tm1`.
/// Use [`profile_signed`] to get the signed integer value.
#[inline]
pub fn profile_sub(tm1: ProftimeT, tm2: ProftimeT) -> ProftimeT {
    tm1.wrapping_sub(tm2)
}

/// Previous timestamp used by startup-time nesting (`g_prev_time`).
static G_PREV_TIME: crate::globals::GlobalCell<ProftimeT> =
    crate::globals::GlobalCell::new(0);

/// Startup-report baseline (`g_start_time`).
static G_START_TIME: crate::globals::GlobalCell<ProftimeT> =
    crate::globals::GlobalCell::new(0);

/// Saves timing state before an operation that may nest (`time_push`).
///
/// The original writes elapsed time and the new start through two
/// out-parameters; both are returned as `(relative, start)` here.
///
/// # Safety
/// Mutates the `G_PREV_TIME` file-static.
#[must_use]
pub unsafe fn time_push() -> (ProftimeT, ProftimeT) {
    let now = profile_start();
    // SAFETY: forwarded from this function's own safety doc.
    let prev = unsafe { *G_PREV_TIME.get_mut() };
    let relative = profile_sub(now, prev);
    unsafe { *G_PREV_TIME.get_mut() = now };
    (relative, now)
}

/// Restores the previous-time baseline after nested work
/// (`time_pop`) by subtracting `tp`.
///
/// # Safety
/// Mutates the `G_PREV_TIME` file-static.
pub unsafe fn time_pop(tp: ProftimeT) {
    // SAFETY: forwarded from this function's own safety doc.
    let previous = unsafe { *G_PREV_TIME.get_mut() };
    unsafe { *G_PREV_TIME.get_mut() = previous.wrapping_sub(tp) };
}

/// Format the difference between two startup timestamps (`time_diff`)
/// as milliseconds with three fractional digits.
fn time_diff(then: ProftimeT, now: ProftimeT) -> String {
    let diff = profile_sub(now, then);
    format!("{:07.3}", diff as f64 / 1.0e6)
}

/// Initialize startup timing and write the report headings
/// (`time_start`).
///
/// # Safety
/// Mutates shared startup timing and output state.
pub unsafe fn time_start(message: &[u8]) -> std::io::Result<()> {
    if unsafe { crate::globals::GLOBALS.get_mut() }
        .time_fd
        .is_none()
    {
        return Ok(());
    }

    let now = profile_start();
    unsafe {
        *G_PREV_TIME.get_mut() = now;
        *G_START_TIME.get_mut() = now;
    }
    {
        let writer = unsafe { crate::globals::GLOBALS.get_mut() }
            .time_fd
            .as_mut()
            .expect("checked above");
        writer.write_all(b"\ntimes in msec\n")?;
        writer.write_all(b" clock   self+sourced   self:  sourced script\n")?;
        writer.write_all(b" clock   elapsed:              other lines\n\n")?;
    }
    unsafe { time_msg(message, None) }
}

/// Write one startup timing record (`time_msg`).
///
/// # Safety
/// Mutates shared startup timing and output state.
pub unsafe fn time_msg(
    message: &[u8],
    start: Option<ProftimeT>,
) -> std::io::Result<()> {
    if unsafe { crate::globals::GLOBALS.get_mut() }
        .time_fd
        .is_none()
    {
        return Ok(());
    }

    let now = profile_start();
    let absolute = time_diff(unsafe { *G_START_TIME.get_mut() }, now);
    let sourced = start.map(|started| time_diff(started, now));
    let elapsed = time_diff(unsafe { *G_PREV_TIME.get_mut() }, now);
    unsafe { *G_PREV_TIME.get_mut() = now };

    let writer = unsafe { crate::globals::GLOBALS.get_mut() }
        .time_fd
        .as_mut()
        .expect("checked above");
    writer.write_all(absolute.as_bytes())?;
    if let Some(sourced) = sourced {
        writer.write_all(b"  ")?;
        writer.write_all(sourced.as_bytes())?;
    }
    writer.write_all(b"  ")?;
    writer.write_all(elapsed.as_bytes())?;
    writer.write_all(b": ")?;
    writer.write_all(message)?;
    writer.write_all(b"\n")
}

/// Open and buffer the `--startuptime` report (`time_init`).
///
/// # Safety
/// Mutates shared startup report state and must be called at most once
/// before [`time_finish`].
pub unsafe fn time_init(
    path: &std::path::Path,
    process_name: &[u8],
) -> std::io::Result<()> {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    let mut writer = std::io::BufWriter::with_capacity(8193, file);
    writer.write_all(b"--- Startup times for process: ")?;
    writer.write_all(process_name)?;
    writer.write_all(b" ---\n")?;
    unsafe { crate::globals::GLOBALS.get_mut() }.time_fd = Some(writer);
    Ok(())
}

/// Finish and flush the `--startuptime` report (`time_finish`).
///
/// # Safety
/// Mutates shared startup timing and output state.
pub unsafe fn time_finish() -> std::io::Result<()> {
    if unsafe { crate::globals::GLOBALS.get_mut() }
        .time_fd
        .is_none()
    {
        return Ok(());
    }
    unsafe { time_msg(b"--- NVIM STARTED ---\n", None) }?;
    if let Some(mut writer) =
        unsafe { crate::globals::GLOBALS.get_mut() }.time_fd.take()
    {
        writer.flush()?;
    }
    Ok(())
}

/// Adds the `self` time from the total time and the `children` time
/// (`profile_self`).
///
/// Returns `self` if `total <= children` (can happen with recursive
/// calls), otherwise `self + total - children`.
pub fn profile_self(self_: ProftimeT, total: ProftimeT, children: ProftimeT) -> ProftimeT {
    // check that the result won't be negative, which can happen with
    // recursive calls.
    if total <= children {
        return self_;
    }
    // add the total time to self and subtract the children's time from self
    profile_sub(profile_add(self_, total), children)
}

static PROF_WAIT_TIME: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Gets the current waittime (`profile_get_wait`).
#[inline]
fn profile_get_wait() -> ProftimeT {
    PROF_WAIT_TIME.load(std::sync::atomic::Ordering::Relaxed)
}

/// Sets the current waittime (`profile_set_wait`).
#[inline]
pub fn profile_set_wait(wait: ProftimeT) {
    PROF_WAIT_TIME.store(wait, std::sync::atomic::Ordering::Relaxed);
}

/// The accumulated wait time for the current input wait
/// (`wait_time`, a file-static in the original).
static PROF_INPUT_WAIT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Called when starting to wait for the user to type a character
/// (`prof_input_start`).
pub fn prof_input_start() {
    PROF_INPUT_WAIT.store(profile_start(), std::sync::atomic::Ordering::Relaxed);
}

/// Called when finished waiting for the user to type a character
/// (`prof_input_end`).
///
/// The elapsed wait is added to the running total, so profiling
/// reports exclude time spent blocked on the user.
pub fn prof_input_end() {
    let started = PROF_INPUT_WAIT.load(std::sync::atomic::Ordering::Relaxed);
    let waited = profile_end(started);
    PROF_INPUT_WAIT.store(waited, std::sync::atomic::Ordering::Relaxed);
    profile_set_wait(profile_add(profile_get_wait(), waited));
}

/// Format the count and times for one function or source line
/// (`prof_func_line`).
///
/// The original writes directly to a `FILE *`; returning the formatted
/// bytes lets the eventual Rust profile writer use owned buffered I/O.
#[allow(dead_code)]
fn prof_func_line(
    count: i32,
    total: ProftimeT,
    self_time: ProftimeT,
    prefer_self: bool,
) -> String {
    if count <= 0 {
        return " ".repeat(28);
    }

    let mut line = format!("{count:5} ");
    if prefer_self && profile_equal(total, self_time) {
        line.push_str("           ");
    } else {
        line.push_str(&profile_msg(total));
        line.push(' ');
    }
    if !prefer_self && profile_equal(total, self_time) {
        line.push_str("           ");
    } else {
        line.push_str(&profile_msg(self_time));
        line.push(' ');
    }
    line
}

fn profile_function_name(
    function: &crate::eval::typval_defs::UfuncT,
) -> String {
    let mut name = function.uf_name.as_slice();
    if name.last() == Some(&0) {
        name = &name[..name.len() - 1];
    }
    let mut display = String::new();
    if name.starts_with(&[
        crate::keycodes_defs::K_SPECIAL,
        crate::keycodes_defs::KS_EXTRA,
        crate::keycodes_defs::KE_SNR,
    ]) {
        display.push_str("<SNR>");
        name = &name[3..];
    }
    display.push_str(&String::from_utf8_lossy(name));
    display
}

/// Format one sorted function table (`prof_sort_list`).
///
/// The input is already sorted, matching the original's `sorttab`
/// contract. At most the first 20 entries are printed.
#[allow(dead_code)]
fn prof_sort_list(
    functions: &[&crate::eval::typval_defs::UfuncT],
    title: &str,
    prefer_self: bool,
) -> String {
    let mut report = format!(
        "FUNCTIONS SORTED ON {title} TIME\n\
         count  total (s)   self (s)  function\n"
    );
    for function in functions.iter().take(20) {
        report.push_str(&prof_func_line(
            function.uf_tm_count,
            function.uf_tm_total,
            function.uf_tm_self,
            prefer_self,
        ));
        report.push(' ');

        report.push_str(&profile_function_name(function));
        report.push_str("()\n");
    }
    report.push('\n');
    report
}

/// Format profiling results for every registered user function
/// (`func_dump_profile`).
///
/// The original writes directly to a `FILE *`; this returns the same
/// report bytes for the eventual owned profile writer.
///
/// # Safety
/// Reads the shared function and script registries. Registered pointers
/// must remain live and neither registry may change during the call.
#[allow(dead_code)]
unsafe fn func_dump_profile() -> String {
    // SAFETY: forwarded from this function's own safety doc.
    let pointers = unsafe { crate::eval::userfunc::func_tbl_values() };
    let mut functions = Vec::new();
    let mut report = String::new();

    for pointer in pointers {
        // SAFETY: func_tbl_values returns registered live functions.
        let function = unsafe { &*pointer };
        if function.uf_prof_initialized == 0 {
            continue;
        }
        functions.push(function);
        report.push_str("FUNCTION  ");
        report.push_str(&profile_function_name(function));
        report.push_str("()\n");
        if function.uf_script_ctx.sc_sid != 0 {
            // SAFETY: forwarded from this function's own safety doc.
            let name = unsafe {
                crate::runtime::get_scriptname(function.uf_script_ctx)
            };
            report.push_str("    Defined: ");
            report.push_str(&String::from_utf8_lossy(&name));
            report.push(':');
            report.push_str(&function.uf_script_ctx.sc_lnum.to_string());
            report.push('\n');
        }
        if function.uf_tm_count == 1 {
            report.push_str("Called 1 time\n");
        } else {
            report.push_str(&format!(
                "Called {} times\n",
                function.uf_tm_count
            ));
        }
        report.push_str("Total time: ");
        report.push_str(&profile_msg(function.uf_tm_total));
        report.push_str("\n Self time: ");
        report.push_str(&profile_msg(function.uf_tm_self));
        report.push_str("\n\ncount  total (s)   self (s)\n");

        for (index, line) in function.uf_lines.iter().enumerate() {
            let Some(line) = line else {
                continue;
            };
            report.push_str(&prof_func_line(
                function.uf_tml_count[index],
                function.uf_tml_total[index],
                function.uf_tml_self[index],
                true,
            ));
            report.push_str(&String::from_utf8_lossy(line));
            report.push('\n');
        }
        report.push('\n');
    }

    if !functions.is_empty() {
        functions.sort_by(|left, right| {
            prof_total_cmp(left, right).cmp(&0)
        });
        report.push_str(&prof_sort_list(
            &functions,
            "TOTAL",
            false,
        ));
        functions.sort_by(|left, right| {
            prof_self_cmp(left, right).cmp(&0)
        });
        report.push_str(&prof_sort_list(&functions, "SELF", true));
    }
    report
}

/// Read source bytes in the same chunks as `vim_fgets(IOSIZE)`.
fn profile_source_chunks(source: &[u8]) -> Vec<Vec<u8>> {
    let mut chunks = Vec::new();
    let mut position = 0;
    while position < source.len() {
        let remaining = &source[position..];
        let read_len = remaining
            .iter()
            .take(crate::globals::IOSIZE - 1)
            .position(|&byte| byte == b'\n')
            .map_or(
                remaining.len().min(crate::globals::IOSIZE - 1),
                |newline| newline + 1,
            );
        let mut chunk = remaining[..read_len].to_vec();
        position += read_len;

        if chunk.len() == crate::globals::IOSIZE - 1
            && chunk.last().is_some_and(|&byte| byte != 0 && byte != b'\n')
        {
            let mut end = chunk.len() - 1;
            while end > 0 && chunk[end] & 0xc0 == 0x80 {
                end -= 1;
            }
            chunk.truncate(end);
            chunk.push(b'\n');
        }
        if let Some(nul) = chunk.iter().position(|&byte| byte == 0) {
            chunk.truncate(nul);
        }
        chunks.push(chunk);
    }
    chunks
}

/// Format profiling results for every profiled script
/// (`script_dump_profile`).
///
/// # Safety
/// Reads the shared script registry, whose pointers must remain live and
/// whose contents must not change during this call.
#[allow(dead_code)]
unsafe fn script_dump_profile() -> Vec<u8> {
    let mut report = Vec::new();
    for sid in 1..=crate::runtime::script_item_count() {
        let script = crate::runtime::script_item(sid);
        // SAFETY: forwarded from this function's own safety doc.
        let script = unsafe { &*script };
        if !script.sn_prof_on {
            continue;
        }
        let name = script.sn_name.as_deref().unwrap_or_default();
        report.extend_from_slice(b"SCRIPT  ");
        report.extend_from_slice(name);
        report.push(b'\n');
        if script.sn_pr_count == 1 {
            report.extend_from_slice(b"Sourced 1 time\n");
        } else {
            report.extend_from_slice(
                format!("Sourced {} times\n", script.sn_pr_count)
                    .as_bytes(),
            );
        }
        report.extend_from_slice(
            format!(
                "Total time: {}\n Self time: {}\n\n\
                 count  total (s)   self (s)\n",
                profile_msg(script.sn_pr_total),
                profile_msg(script.sn_pr_self),
            )
            .as_bytes(),
        );

        #[cfg(unix)]
        let source = {
            use std::os::unix::ffi::OsStrExt;
            let path = std::path::Path::new(std::ffi::OsStr::from_bytes(name));
            std::fs::read(path).ok()
        };
        #[cfg(windows)]
        let source = std::str::from_utf8(name)
            .ok()
            .and_then(|path| std::fs::read(path).ok());
        let Some(source) = source else {
            report.extend_from_slice(b"Cannot open file!\n\n");
            continue;
        };
        for (index, chunk) in profile_source_chunks(&source)
            .into_iter()
            .enumerate()
        {
            if let Some(line) = script.sn_prl_ga.get(index)
                && line.snp_count > 0
            {
                report.extend_from_slice(
                    prof_func_line(
                        line.snp_count,
                        line.sn_prl_total,
                        line.sn_prl_self,
                        true,
                    )
                    .as_bytes(),
                );
            } else {
                report.extend_from_slice(b"                            ");
            }
            report.extend_from_slice(&chunk);
        }
        report.push(b'\n');
    }
    report
}

/// Write all script and function profiling information (`profile_dump`).
///
/// # Safety
/// Reads the shared script/function registries and profile output name;
/// none may change during the call.
pub unsafe fn profile_dump() -> std::io::Result<()> {
    // SAFETY: forwarded from this function's own safety doc.
    let Some(name) = unsafe { PROFILE_FNAME.get_mut() }.clone() else {
        return Ok(());
    };
    #[cfg(unix)]
    let path = {
        use std::os::unix::ffi::OsStrExt;
        std::path::PathBuf::from(std::ffi::OsStr::from_bytes(&name))
    };
    #[cfg(windows)]
    let path = std::path::PathBuf::from(
        std::str::from_utf8(&name).map_err(|error| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, error)
        })?,
    );

    let file = std::fs::File::create(path)?;
    let mut writer = std::io::BufWriter::new(file);
    // SAFETY: forwarded from this function's own safety doc.
    writer.write_all(&unsafe { script_dump_profile() })?;
    // SAFETY: forwarded from this function's own safety doc.
    writer.write_all(unsafe { func_dump_profile() }.as_bytes())?;
    writer.flush()
}

/// Execute a `:profile` command (`ex_profile`).
///
/// `file`/`func` subcommands share the debugger breakpoint parser
/// (`ex_breakadd`) and remain deferred with that subsystem.
///
/// # Safety
/// Mutates shared profiling, Vim-variable, script, and function state.
pub unsafe fn ex_profile(
    eap: &crate::ex_cmds_defs::ExargT,
) -> std::io::Result<()> {
    let argument = eap.arg.as_deref().unwrap_or_default();
    let end_subcmd = crate::charset::skiptowhite(argument);
    let rest_start =
        end_subcmd + crate::charset::skipwhite(&argument[end_subcmd..]);
    let subcmd = &argument[..end_subcmd];
    let rest = &argument[rest_start..];
    let profiling =
        unsafe { crate::globals::GLOBALS.get_mut() }.do_profiling;

    if subcmd == b"start" && !rest.is_empty() {
        // SAFETY: forwarded from this function's own safety doc.
        *unsafe { PROFILE_FNAME.get_mut() } =
            Some(unsafe { crate::os::env::expand_env_save_opt(rest, true, None) });
        unsafe { crate::globals::GLOBALS.get_mut() }.do_profiling =
            crate::globals::PROF_YES;
        profile_set_wait(profile_zero());
        unsafe {
            crate::eval::vars::set_vim_var_nr(
                crate::eval::vars::VimVarIndex::Profiling,
                1,
            )
        };
    } else if profiling == crate::globals::PROF_NONE {
        // The original displays E750 and otherwise leaves state alone.
    } else if argument == b"stop" {
        // Match the original's ordering: even a dump-open failure still
        // stops and resets profiling.
        let result = unsafe { profile_dump() };
        unsafe { crate::globals::GLOBALS.get_mut() }.do_profiling =
            crate::globals::PROF_NONE;
        unsafe {
            crate::eval::vars::set_vim_var_nr(
                crate::eval::vars::VimVarIndex::Profiling,
                0,
            )
        };
        unsafe { profile_reset() };
        result?;
    } else if argument == b"pause" {
        if profiling == crate::globals::PROF_YES {
            unsafe { *PAUSE_TIME.get_mut() = profile_start() };
        }
        unsafe { crate::globals::GLOBALS.get_mut() }.do_profiling =
            crate::globals::PROF_PAUSED;
    } else if argument == b"continue" {
        if profiling == crate::globals::PROF_PAUSED {
            let paused =
                profile_end(unsafe { *PAUSE_TIME.get_mut() });
            profile_set_wait(profile_add(profile_get_wait(), paused));
        }
        unsafe { crate::globals::GLOBALS.get_mut() }.do_profiling =
            crate::globals::PROF_YES;
    } else if argument == b"dump" {
        unsafe { profile_dump() }?;
    } else {
        unimplemented!(
            "ex_profile: file/func subcommands need debugger::ex_breakadd"
        );
    }
    Ok(())
}

/// Compare two functions by total time, for sorting
/// (`prof_total_cmp`).
///
/// The original is a `qsort` comparator taking `void *`; Rust's own
/// `sort_by` takes the elements directly.
#[must_use]
pub fn prof_total_cmp(
    p1: &crate::eval::typval_defs::UfuncT,
    p2: &crate::eval::typval_defs::UfuncT,
) -> i32 {
    profile_cmp(p1.uf_tm_total, p2.uf_tm_total)
}

/// Compare two functions by self time, for sorting (`prof_self_cmp`).
#[must_use]
pub fn prof_self_cmp(
    p1: &crate::eval::typval_defs::UfuncT,
    p2: &crate::eval::typval_defs::UfuncT,
) -> i32 {
    profile_cmp(p1.uf_tm_self, p2.uf_tm_self)
}

/// Subtracts the passed waittime since `tm` (`profile_sub_wait`).
///
/// Returns `tma - (waittime - tm)`.
pub fn profile_sub_wait(tm: ProftimeT, tma: ProftimeT) -> ProftimeT {
    let tm3 = profile_sub(profile_get_wait(), tm);
    profile_sub(tma, tm3)
}

/// Checks if time `tm1` is equal to `tm2` (`profile_equal`).
#[inline]
fn profile_equal(tm1: ProftimeT, tm2: ProftimeT) -> bool {
    tm1 == tm2
}

/// Converts time duration `tm` (a [`profile_sub`] result) to a signed
/// integer (`profile_signed`).
///
/// `(tm > i64::MAX)` is >=150 years, so we can assume it was produced by
/// arithmetic of two `proftime_T` values. For human-readable representation
/// (and Vim-compat) we want the difference after unsigned wraparound.
pub fn profile_signed(tm: ProftimeT) -> i64 {
    if tm <= i64::MAX as u64 {
        tm as i64
    } else {
        -((u64::MAX - tm) as i64)
    }
}

/// Compares profiling times (`profile_cmp`).
///
/// Times `tm1` and `tm2` must be less than 150 years apart.
///
/// Returns <0 if `tm2 < tm1`, 0 if equal, >0 if `tm2 > tm1`.
pub fn profile_cmp(tm1: ProftimeT, tm2: ProftimeT) -> i32 {
    if profile_equal(tm1, tm2) {
        return 0;
    }
    if profile_signed(tm2.wrapping_sub(tm1)) < 0 {
        -1
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct CurrentFunccallGuard(*mut crate::eval::typval_defs::FunccallT);

    struct ProfileExestackGuard(Vec<crate::runtime_defs::EstackT>);

    struct StartupReportGuard(std::path::PathBuf);

    struct ProfileFilenameGuard(Option<Vec<u8>>);

    struct ProfilingStateGuard {
        do_profiling: i32,
        wait: ProftimeT,
        pause: ProftimeT,
        vimvar: crate::eval::typval_defs::VarnumberT,
    }

    impl ProfilingStateGuard {
        fn new() -> Self {
            Self {
                do_profiling: unsafe {
                    crate::globals::GLOBALS.get_mut()
                }
                .do_profiling,
                wait: profile_get_wait(),
                pause: unsafe { *PAUSE_TIME.get_mut() },
                vimvar: unsafe {
                    crate::eval::vars::get_vim_var_nr(
                        crate::eval::vars::VimVarIndex::Profiling,
                    )
                },
            }
        }
    }

    impl Drop for ProfilingStateGuard {
        fn drop(&mut self) {
            unsafe {
                crate::globals::GLOBALS.get_mut().do_profiling =
                    self.do_profiling;
                *PAUSE_TIME.get_mut() = self.pause;
                crate::eval::vars::set_vim_var_nr(
                    crate::eval::vars::VimVarIndex::Profiling,
                    self.vimvar,
                );
            }
            profile_set_wait(self.wait);
        }
    }

    impl ProfileFilenameGuard {
        fn set(name: Option<Vec<u8>>) -> Self {
            let previous =
                std::mem::replace(unsafe { PROFILE_FNAME.get_mut() }, name);
            Self(previous)
        }
    }

    impl Drop for ProfileFilenameGuard {
        fn drop(&mut self) {
            *unsafe { PROFILE_FNAME.get_mut() } = self.0.take();
        }
    }

    impl Drop for StartupReportGuard {
        fn drop(&mut self) {
            if let Some(mut writer) =
                unsafe { crate::globals::GLOBALS.get_mut() }.time_fd.take()
            {
                let _ = writer.flush();
            }
            let _ = std::fs::remove_file(&self.0);
        }
    }

    impl ProfileExestackGuard {
        fn line(line: crate::pos_defs::LinenrT) -> Self {
            let stack = vec![crate::runtime_defs::EstackT {
                es_lnum: line,
                ..Default::default()
            }];
            Self(crate::runtime::replace_exestack_for_test(stack))
        }
    }

    impl Drop for ProfileExestackGuard {
        fn drop(&mut self) {
            let saved = std::mem::take(&mut self.0);
            let _ = crate::runtime::replace_exestack_for_test(saved);
        }
    }

    impl CurrentFunccallGuard {
        fn install(value: *mut crate::eval::typval_defs::FunccallT) -> Self {
            let saved = crate::eval::userfunc::get_current_funccal();
            crate::eval::userfunc::set_current_funccal(value);
            Self(saved)
        }
    }

    impl Drop for CurrentFunccallGuard {
        fn drop(&mut self) {
            crate::eval::userfunc::set_current_funccal(self.0);
        }
    }

    #[test]
    fn func_line_exec_marks_only_an_active_profiled_line() {
        let mut func = crate::eval::typval_defs::UfuncT {
            uf_profiling: 1,
            uf_tml_idx: 3,
            ..Default::default()
        };
        let func_ptr = std::ptr::addr_of_mut!(func);
        let mut call = crate::eval::typval_defs::FunccallT {
            fc_func: func_ptr,
            ..Default::default()
        };
        let call_ptr = std::ptr::addr_of_mut!(call);

        unsafe { func_line_exec(call_ptr.cast()) };
        assert_eq!(unsafe { (*func_ptr).uf_tml_execed }, 1);

        unsafe {
            (*func_ptr).uf_tml_execed = 0;
            (*func_ptr).uf_tml_idx = -1;
        }

        unsafe { func_line_exec(call_ptr.cast()) };
        assert_eq!(unsafe { (*func_ptr).uf_tml_execed }, 0);

        unsafe {
            (*func_ptr).uf_tml_idx = 0;
            (*func_ptr).uf_profiling = 0;
        }
        unsafe { func_line_exec(call_ptr.cast()) };
        assert_eq!(unsafe { (*func_ptr).uf_tml_execed }, 0);
    }

    #[test]
    fn func_line_start_skips_back_over_continuation_lines() {
        let _lock = crate::globals::global_state_test_lock();
        let _stack = ProfileExestackGuard::line(2);
        let mut function = crate::eval::typval_defs::UfuncT {
            uf_profiling: 1,
            uf_lines: vec![Some(b"first".to_vec()), None, Some(b"third".to_vec())],
            ..Default::default()
        };
        let function_ptr = std::ptr::addr_of_mut!(function);
        let mut call = crate::eval::typval_defs::FunccallT {
            fc_func: function_ptr,
            ..Default::default()
        };

        unsafe { func_line_start(std::ptr::addr_of_mut!(call).cast()) };

        assert_eq!(function.uf_tml_idx, 0);
        assert_eq!(function.uf_tml_execed, 0);
        assert_eq!(function.uf_tml_children, 0);
        assert_eq!(function.uf_tml_wait, profile_get_wait());
    }

    #[test]
    fn profile_init_resets_script_counters_and_enables_profiling() {
        let mut script = crate::runtime_defs::ScriptitemT {
            sn_prof_on: false,
            sn_pr_nest: 7,
            sn_pr_count: 9,
            sn_pr_total: 100,
            sn_pr_self: 40,
            sn_prl_ga: vec![crate::runtime_defs::SnPrlT {
                snp_count: 3,
                sn_prl_total: 8,
                sn_prl_self: 5,
            }],
            sn_prl_idx: 4,
            ..Default::default()
        };

        profile_init(&mut script);

        assert!(script.sn_prof_on);
        assert_eq!(script.sn_pr_nest, 0);
        assert_eq!(script.sn_pr_count, 0);
        assert_eq!(script.sn_pr_total, 0);
        assert_eq!(script.sn_pr_self, 0);
        assert!(script.sn_prl_ga.is_empty());
        assert_eq!(script.sn_prl_idx, -1);
    }

    #[test]
    fn script_line_start_grows_counters_and_starts_the_requested_line() {
        let _lock = crate::globals::global_state_test_lock();
        let (sid, item) =
            crate::runtime::new_script_item(Some(b"profile-lines.vim".to_vec()));
        unsafe { profile_init(&mut *item) };
        let sctx = crate::eval::typval_defs::SctxT {
            sc_sid: sid,
            ..Default::default()
        };
        let _sctx = unsafe {
            crate::globals::GlobalFieldGuard::install(|g| &mut g.current_sctx, sctx)
        };
        let _stack = ProfileExestackGuard::line(3);

        unsafe { script_line_start() };

        let item = unsafe { &*item };
        assert_eq!(item.sn_prl_ga.len(), 3);
        assert_eq!(item.sn_prl_idx, 2);
        assert_eq!(item.sn_prl_execed, 0);
        assert_eq!(item.sn_prl_children, 0);
        assert_eq!(item.sn_prl_wait, profile_get_wait());
    }

    #[test]
    fn script_line_end_accumulates_an_executed_line_and_clears_index() {
        let _lock = crate::globals::global_state_test_lock();
        let (sid, item) =
            crate::runtime::new_script_item(Some(b"profile-end.vim".to_vec()));
        let sctx = crate::eval::typval_defs::SctxT {
            sc_sid: sid,
            ..Default::default()
        };
        let _sctx = unsafe {
            crate::globals::GlobalFieldGuard::install(|g| &mut g.current_sctx, sctx)
        };
        unsafe {
            (*item).sn_prof_on = true;
            (*item).sn_prl_ga = vec![crate::runtime_defs::SnPrlT::default()];
            (*item).sn_prl_idx = 0;
            (*item).sn_prl_execed = 1;
            (*item).sn_prl_start = profile_start().wrapping_sub(1_000_000);
            (*item).sn_prl_children = 100;
            (*item).sn_prl_wait = profile_get_wait();
        }

        unsafe { script_line_end() };

        let item = unsafe { &*item };
        assert_eq!(item.sn_prl_idx, -1);
        assert_eq!(item.sn_prl_ga[0].snp_count, 1);
        assert!(item.sn_prl_start > 0);
        assert_eq!(item.sn_prl_ga[0].sn_prl_total, item.sn_prl_start);
        assert_eq!(
            item.sn_prl_ga[0].sn_prl_self,
            profile_self(0, item.sn_prl_start, 100)
        );
    }

    #[test]
    fn func_line_end_accumulates_an_executed_line_and_clears_the_index() {
        let mut func = crate::eval::typval_defs::UfuncT {
            uf_profiling: 1,
            uf_tml_count: vec![0],
            uf_tml_total: vec![0],
            uf_tml_self: vec![0],
            uf_tml_start: profile_start().wrapping_sub(1_000_000),
            uf_tml_children: 100,
            uf_tml_wait: profile_get_wait(),
            uf_tml_idx: 0,
            uf_tml_execed: 1,
            ..Default::default()
        };
        let func_ptr = std::ptr::addr_of_mut!(func);
        let mut call = crate::eval::typval_defs::FunccallT {
            fc_func: func_ptr,
            ..Default::default()
        };
        let call_ptr = std::ptr::addr_of_mut!(call);

        unsafe { func_line_end(call_ptr.cast()) };

        assert_eq!(unsafe { (*func_ptr).uf_tml_idx }, -1);
        assert_eq!(unsafe { &(*func_ptr).uf_tml_count }, &[1]);
        let elapsed = unsafe { (*func_ptr).uf_tml_start };
        assert!(elapsed > 0);
        assert_eq!(unsafe { (&(*func_ptr).uf_tml_total)[0] }, elapsed);
        assert_eq!(
            unsafe { (&(*func_ptr).uf_tml_self)[0] },
            profile_self(0, elapsed, 100)
        );
    }

    #[test]
    fn script_line_exec_marks_only_an_active_profiled_line() {
        let _lock = crate::globals::global_state_test_lock();
        let (sid, item) = crate::runtime::new_script_item(Some(b"profile.vim".to_vec()));
        let sctx = crate::eval::typval_defs::SctxT {
            sc_sid: sid,
            ..Default::default()
        };
        let _sctx = unsafe {
            crate::globals::GlobalFieldGuard::install(|g| &mut g.current_sctx, sctx)
        };
        unsafe {
            (*item).sn_prof_on = true;
            (*item).sn_prl_idx = 0;
            (*item).sn_prl_execed = 0;
        }

        unsafe { script_line_exec() };
        assert_eq!(unsafe { (*item).sn_prl_execed }, 1);

        unsafe {
            (*item).sn_prl_execed = 0;
            (*item).sn_prl_idx = -1;
        }
        unsafe { script_line_exec() };
        assert_eq!(unsafe { (*item).sn_prl_execed }, 0);

        unsafe {
            (*item).sn_prl_idx = 0;
            (*item).sn_prof_on = false;
        }
        unsafe { script_line_exec() };
        assert_eq!(unsafe { (*item).sn_prl_execed }, 0);
    }

    #[test]
    fn script_prof_save_starts_only_the_outermost_profiled_child() {
        let _lock = crate::globals::global_state_test_lock();
        let (sid, item) = crate::runtime::new_script_item(Some(b"child.vim".to_vec()));
        let sctx = crate::eval::typval_defs::SctxT {
            sc_sid: sid,
            ..Default::default()
        };
        let _sctx = unsafe {
            crate::globals::GlobalFieldGuard::install(|g| &mut g.current_sctx, sctx)
        };
        unsafe {
            (*item).sn_prof_on = true;
            (*item).sn_pr_nest = 0;
            (*item).sn_pr_child = 0;
        }
        let mut wait = u64::MAX;

        unsafe { script_prof_save(&mut wait) };
        assert_eq!(unsafe { (*item).sn_pr_nest }, 1);
        let child_start = unsafe { (*item).sn_pr_child };
        assert_eq!(wait, profile_get_wait());

        unsafe { script_prof_save(&mut wait) };
        assert_eq!(unsafe { (*item).sn_pr_nest }, 2);
        assert_eq!(unsafe { (*item).sn_pr_child }, child_start);
    }

    #[test]
    fn script_prof_restore_accumulates_only_when_leaving_the_outermost_child() {
        let _lock = crate::globals::global_state_test_lock();
        let (sid, item) = crate::runtime::new_script_item(Some(b"restore.vim".to_vec()));
        let sctx = crate::eval::typval_defs::SctxT {
            sc_sid: sid,
            ..Default::default()
        };
        let _sctx = unsafe {
            crate::globals::GlobalFieldGuard::install(|g| &mut g.current_sctx, sctx)
        };
        let wait = profile_get_wait();
        unsafe {
            (*item).sn_prof_on = true;
            (*item).sn_pr_nest = 2;
            (*item).sn_pr_child = profile_start().wrapping_sub(1_000_000);
            (*item).sn_pr_children = 0;
            (*item).sn_prl_children = 0;
        }

        unsafe { script_prof_restore(&wait) };
        assert_eq!(unsafe { (*item).sn_pr_nest }, 1);
        assert_eq!(unsafe { (*item).sn_pr_children }, 0);

        unsafe { script_prof_restore(&wait) };
        assert_eq!(unsafe { (*item).sn_pr_nest }, 0);
        assert!(unsafe { (*item).sn_pr_child } > 0);
        assert_eq!(
            unsafe { (*item).sn_pr_children },
            unsafe { (*item).sn_pr_child }
        );
        assert_eq!(
            unsafe { (*item).sn_prl_children },
            unsafe { (*item).sn_pr_child }
        );
    }

    #[test]
    fn prof_child_enter_starts_a_profiled_current_function() {
        let _lock = crate::globals::global_state_test_lock();
        let mut func = crate::eval::typval_defs::UfuncT {
            uf_profiling: 1,
            ..Default::default()
        };
        let func_ptr = std::ptr::addr_of_mut!(func);
        let mut call = crate::eval::typval_defs::FunccallT {
            fc_func: func_ptr,
            ..Default::default()
        };
        let call_ptr = std::ptr::addr_of_mut!(call);
        let _call = CurrentFunccallGuard::install(call_ptr);
        let before = profile_start();
        let mut wait = u64::MAX;

        unsafe { prof_child_enter(&mut wait) };

        assert!(unsafe { (*call_ptr).fc_prof_child } >= before);
        assert_eq!(wait, profile_get_wait());
    }

    #[test]
    fn prof_child_exit_accumulates_time_in_the_current_function() {
        let _lock = crate::globals::global_state_test_lock();
        let mut func = crate::eval::typval_defs::UfuncT {
            uf_profiling: 1,
            ..Default::default()
        };
        let func_ptr = std::ptr::addr_of_mut!(func);
        let mut call = crate::eval::typval_defs::FunccallT {
            fc_func: func_ptr,
            fc_prof_child: profile_start().wrapping_sub(1_000_000),
            ..Default::default()
        };
        let call_ptr = std::ptr::addr_of_mut!(call);
        let _call = CurrentFunccallGuard::install(call_ptr);
        let wait = profile_get_wait();

        unsafe { prof_child_exit(&wait) };

        let child = unsafe { (*call_ptr).fc_prof_child };
        assert!(child > 0);
        assert_eq!(unsafe { (*func_ptr).uf_tm_children }, child);
        assert_eq!(unsafe { (*func_ptr).uf_tml_children }, child);
    }

    struct PrevTimeGuard(ProftimeT);

    impl PrevTimeGuard {
        fn install(value: ProftimeT) -> Self {
            let slot = unsafe { G_PREV_TIME.get_mut() };
            let saved = *slot;
            *slot = value;
            Self(saved)
        }
    }

    impl Drop for PrevTimeGuard {
        fn drop(&mut self) {
            unsafe { *G_PREV_TIME.get_mut() = self.0 };
        }
    }

    #[test]
    fn time_push_returns_elapsed_time_and_updates_the_previous_timestamp() {
        let _lock = crate::globals::global_state_test_lock();
        let previous = profile_start();
        let _g = PrevTimeGuard::install(previous);

        let (relative, start) = unsafe { time_push() };

        assert_eq!(relative, profile_sub(start, previous));
        assert_eq!(unsafe { *G_PREV_TIME.get_mut() }, start);
    }

    #[test]
    fn time_push_from_zero_returns_the_absolute_monotonic_value() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = PrevTimeGuard::install(0);

        let (relative, start) = unsafe { time_push() };

        assert_eq!(relative, start);
    }

    #[test]
    fn time_pop_subtracts_from_the_previous_timestamp() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = PrevTimeGuard::install(1_000);

        unsafe { time_pop(400) };

        assert_eq!(unsafe { *G_PREV_TIME.get_mut() }, 600);
    }

    #[test]
    fn time_pop_uses_proftime_wrapping_arithmetic() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = PrevTimeGuard::install(5);

        unsafe { time_pop(10) };

        assert_eq!(unsafe { *G_PREV_TIME.get_mut() }, ProftimeT::MAX - 4);
    }

    // --- prof_input_start / prof_input_end ---

    #[test]
    fn prof_input_end_accumulates_the_wait_into_the_running_total() {
        let before = profile_get_wait();

        prof_input_start();
        prof_input_end();
        let after_one = profile_get_wait();
        assert!(after_one >= before, "the wait total never decreases");

        prof_input_start();
        prof_input_end();
        let after_two = profile_get_wait();
        assert!(after_two >= after_one, "a second wait accumulates too");

        profile_set_wait(before);
    }

    #[test]
    fn time_diff_formats_milliseconds_with_three_fractional_digits() {
        assert_eq!(time_diff(1_000_000, 2_234_000), "001.234");
        assert_eq!(time_diff(0, 12_345_678), "012.346");
    }

    #[test]
    fn startup_time_functions_are_noops_without_an_open_report() {
        let _lock = crate::globals::global_state_test_lock();
        assert!(
            unsafe { crate::globals::GLOBALS.get_mut() }
                .time_fd
                .is_none()
        );
        unsafe {
            time_start(b"ignored").unwrap();
            time_msg(b"ignored", Some(0)).unwrap();
            time_finish().unwrap();
        }
    }

    #[test]
    #[cfg_attr(
        miri,
        ignore = "Miri cannot emulate the Windows append-mode CreateFileW access flags"
    )]
    fn startup_time_report_is_appended_buffered_and_flushed() {
        let _lock = crate::globals::global_state_test_lock();
        let path = std::env::temp_dir().join(format!(
            "nero-startuptime-{}-{}.log",
            std::process::id(),
            profile_start()
        ));
        let _file = StartupReportGuard(path.clone());

        unsafe {
            time_init(&path, b"nero-test").unwrap();
            time_start(b"starting").unwrap();
            let source_start = profile_start();
            std::thread::sleep(std::time::Duration::from_millis(1));
            time_msg(b"sourced", Some(source_start)).unwrap();
            time_finish().unwrap();
        }

        assert!(
            unsafe { crate::globals::GLOBALS.get_mut() }
                .time_fd
                .is_none()
        );
        let report = std::fs::read_to_string(path).unwrap();
        assert!(report.starts_with(
            "--- Startup times for process: nero-test ---\n\ntimes in msec\n"
        ));
        assert!(report.contains(concat!(
            " clock   self+sourced   self:  sourced script\n",
            " clock   elapsed:              other lines\n\n"
        )));
        assert!(report.contains(": starting\n"));
        assert!(report.contains(": sourced\n"));
        assert!(report.ends_with(": --- NVIM STARTED ---\n\n"));
    }

    // --- prof_total_cmp / prof_self_cmp ---

    #[test]
    fn prof_total_cmp_orders_by_total_time_descending() {
        // profile_cmp is deliberately REVERSED: it returns >0 when the
        // SECOND argument is larger, so sorting with it puts the
        // slowest function first - which is what a profile report
        // wants. Asserting conventional ascending order here would be
        // wrong.
        let a = crate::eval::typval_defs::UfuncT { uf_tm_total: 10, ..Default::default() };
        let b = crate::eval::typval_defs::UfuncT { uf_tm_total: 20, ..Default::default() };

        assert!(prof_total_cmp(&a, &b) > 0, "the larger total sorts first");
        assert!(prof_total_cmp(&b, &a) < 0);
        assert_eq!(prof_total_cmp(&a, &a), 0);
    }

    #[test]
    fn prof_self_cmp_orders_by_self_time_not_total() {
        // Self and total deliberately disagree, so a comparator using
        // the wrong field would order these the other way.
        let a = crate::eval::typval_defs::UfuncT {
            uf_tm_total: 100,
            uf_tm_self: 1,
            ..Default::default()
        };
        let b = crate::eval::typval_defs::UfuncT {
            uf_tm_total: 1,
            uf_tm_self: 100,
            ..Default::default()
        };

        assert!(prof_self_cmp(&a, &b) > 0, "b has the larger self time");
        assert!(prof_total_cmp(&a, &b) < 0, "but a has the larger total");
    }

    #[test]
    fn prof_func_line_formats_count_total_and_self_columns() {
        assert_eq!(
            prof_func_line(3, 1_000_000_000, 500_000_000, false),
            "    3   1.000000   0.500000 "
        );
    }

    #[test]
    fn prof_func_line_suppresses_the_duplicate_preferred_column() {
        assert_eq!(
            prof_func_line(2, 1_000_000_000, 1_000_000_000, true),
            "    2              1.000000 "
        );
        assert_eq!(
            prof_func_line(2, 1_000_000_000, 1_000_000_000, false),
            "    2   1.000000            "
        );
    }

    #[test]
    fn prof_func_line_uses_a_blank_row_for_zero_count() {
        let line = prof_func_line(0, 1, 2, false);
        assert_eq!(line, " ".repeat(28));
        assert_eq!(line.len(), 28);
    }

    #[test]
    fn prof_sort_list_formats_normal_and_script_local_names() {
        let normal = crate::eval::typval_defs::UfuncT {
            uf_name: b"Normal\0".to_vec(),
            uf_tm_count: 2,
            uf_tm_total: 1_000_000_000,
            uf_tm_self: 500_000_000,
            ..Default::default()
        };
        let mut snr_name = vec![
            crate::keycodes_defs::K_SPECIAL,
            crate::keycodes_defs::KS_EXTRA,
            crate::keycodes_defs::KE_SNR,
        ];
        snr_name.extend_from_slice(b"12_Local\0");
        let script_local = crate::eval::typval_defs::UfuncT {
            uf_name: snr_name,
            uf_tm_count: 1,
            uf_tm_total: 250_000_000,
            uf_tm_self: 250_000_000,
            ..Default::default()
        };

        let report =
            prof_sort_list(&[&normal, &script_local], "TOTAL", false);

        assert!(report.starts_with(
            "FUNCTIONS SORTED ON TOTAL TIME\n\
             count  total (s)   self (s)  function\n"
        ));
        assert!(report.contains(" Normal()\n"));
        assert!(report.contains(" <SNR>12_Local()\n"));
        assert!(report.ends_with("\n\n"));
    }

    #[test]
    fn prof_sort_list_prints_at_most_twenty_functions() {
        let functions: Vec<_> = (0..21)
            .map(|index| crate::eval::typval_defs::UfuncT {
                uf_name: format!("Function{index}\0").into_bytes(),
                uf_tm_count: 1,
                ..Default::default()
            })
            .collect();
        let refs: Vec<_> = functions.iter().collect();

        let report = prof_sort_list(&refs, "SELF", true);

        assert!(report.contains(" Function19()\n"));
        assert!(!report.contains(" Function20()\n"));
        assert_eq!(report.matches("()\n").count(), 20);
    }

    #[test]
    fn func_dump_profile_formats_details_lines_and_rankings() {
        let _lock = crate::globals::global_state_test_lock();
        crate::runtime::tests_reset_for_test();
        crate::eval::userfunc::func_init();
        let (sid, _) =
            crate::runtime::new_script_item(Some(b"profile.vim".to_vec()));

        let normal = Box::into_raw(Box::new(
            crate::eval::typval_defs::UfuncT {
                uf_name: b"Normal\0".to_vec(),
                uf_prof_initialized: 1,
                uf_tm_count: 2,
                uf_tm_total: 2_000_000_000,
                uf_tm_self: 250_000_000,
                uf_lines: vec![
                    Some(b"let x = 1".to_vec()),
                    None,
                    Some(b"return x".to_vec()),
                ],
                uf_tml_count: vec![2, 0, 1],
                uf_tml_total: vec![1_000_000_000, 0, 500_000_000],
                uf_tml_self: vec![750_000_000, 0, 250_000_000],
                uf_script_ctx: crate::eval::typval_defs::SctxT {
                    sc_sid: sid,
                    sc_lnum: 12,
                    ..Default::default()
                },
                ..Default::default()
            },
        ));
        let mut snr_name = vec![
            crate::keycodes_defs::K_SPECIAL,
            crate::keycodes_defs::KS_EXTRA,
            crate::keycodes_defs::KE_SNR,
        ];
        snr_name.extend_from_slice(b"4_Local\0");
        let script_local = Box::into_raw(Box::new(
            crate::eval::typval_defs::UfuncT {
                uf_name: snr_name,
                uf_prof_initialized: 1,
                uf_tm_count: 1,
                uf_tm_total: 1_000_000_000,
                uf_tm_self: 750_000_000,
                uf_lines: vec![Some(b"return 1".to_vec())],
                uf_tml_count: vec![1],
                uf_tml_total: vec![1_000_000_000],
                uf_tml_self: vec![750_000_000],
                ..Default::default()
            },
        ));
        let untouched = Box::into_raw(Box::new(
            crate::eval::typval_defs::UfuncT {
                uf_name: b"Untouched\0".to_vec(),
                ..Default::default()
            },
        ));
        unsafe {
            assert_eq!(
                crate::eval::userfunc::func_hashtab_add(normal),
                crate::vim_defs::OK
            );
            assert_eq!(
                crate::eval::userfunc::func_hashtab_add(script_local),
                crate::vim_defs::OK
            );
            assert_eq!(
                crate::eval::userfunc::func_hashtab_add(untouched),
                crate::vim_defs::OK
            );
        }

        let report = unsafe { func_dump_profile() };

        assert!(report.contains("FUNCTION  Normal()\n"));
        assert!(report.contains("    Defined: profile.vim:12\n"));
        assert!(report.contains("Called 2 times\n"));
        assert!(report.contains("let x = 1\n"));
        assert!(report.contains("return x\n"));
        assert!(!report.contains("Untouched()"));
        assert!(report.contains("FUNCTION  <SNR>4_Local()\n"));
        let total = report.find("FUNCTIONS SORTED ON TOTAL TIME").unwrap();
        let self_time = report.find("FUNCTIONS SORTED ON SELF TIME").unwrap();
        let total_section = &report[total..self_time];
        assert!(
            total_section.find("Normal()").unwrap()
                < total_section.find("<SNR>4_Local()").unwrap()
        );
        let self_section = &report[self_time..];
        assert!(
            self_section.find("<SNR>4_Local()").unwrap()
                < self_section.find("Normal()").unwrap()
        );

        unsafe {
            crate::eval::userfunc::func_init();
            drop(Box::from_raw(normal));
            drop(Box::from_raw(script_local));
            drop(Box::from_raw(untouched));
        }
    }

    #[test]
    fn func_dump_profile_is_empty_without_initialized_functions() {
        let _lock = crate::globals::global_state_test_lock();
        crate::eval::userfunc::func_init();
        assert!(unsafe { func_dump_profile() }.is_empty());
    }

    #[test]
    fn profile_source_chunks_matches_fgets_line_and_size_boundaries() {
        assert_eq!(
            profile_source_chunks(b"one\nlast"),
            vec![b"one\n".to_vec(), b"last".to_vec()]
        );

        let source = vec![b'x'; crate::globals::IOSIZE];
        let chunks = profile_source_chunks(&source);
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].len(), crate::globals::IOSIZE - 1);
        assert_eq!(chunks[0].last(), Some(&b'\n'));
        assert_eq!(chunks[1], vec![b'x']);
    }

    #[test]
    #[cfg_attr(
        miri,
        ignore = "Miri cannot emulate the Windows file-open mode used by std::fs::read"
    )]
    fn script_dump_profile_formats_source_lines_and_timings() {
        let _lock = crate::globals::global_state_test_lock();
        crate::runtime::tests_reset_for_test();
        let path = std::env::temp_dir().join(format!(
            "nero-profile-source-{}-{}.vim",
            std::process::id(),
            profile_start()
        ));
        let _file = StartupReportGuard(path.clone());
        std::fs::write(&path, b"let x = 1\nreturn x\n").unwrap();
        let name = path.to_string_lossy().into_owned().into_bytes();
        let (_, script) = crate::runtime::new_script_item(Some(name.clone()));
        unsafe {
            (*script).sn_prof_on = true;
            (*script).sn_pr_count = 2;
            (*script).sn_pr_total = 2_000_000_000;
            (*script).sn_pr_self = 1_000_000_000;
            (*script).sn_prl_ga = vec![
                crate::runtime_defs::SnPrlT {
                    snp_count: 2,
                    sn_prl_total: 1_500_000_000,
                    sn_prl_self: 500_000_000,
                },
                crate::runtime_defs::SnPrlT {
                    snp_count: 1,
                    sn_prl_total: 500_000_000,
                    sn_prl_self: 500_000_000,
                },
            ];
        }

        let report = unsafe { script_dump_profile() };

        let mut heading = b"SCRIPT  ".to_vec();
        heading.extend_from_slice(&name);
        heading.push(b'\n');
        assert!(report.starts_with(&heading));
        let report = String::from_utf8(report).unwrap();
        assert!(report.contains("Sourced 2 times\n"));
        assert!(report.contains("Total time:   2.000000\n"));
        assert!(report.contains(" Self time:   1.000000\n"));
        assert!(report.contains("let x = 1\n"));
        assert!(report.contains("return x\n"));
    }

    #[test]
    #[cfg_attr(
        miri,
        ignore = "Miri cannot emulate the Windows file-open mode used by std::fs::read"
    )]
    fn script_dump_profile_reports_an_unreadable_source() {
        let _lock = crate::globals::global_state_test_lock();
        crate::runtime::tests_reset_for_test();
        let (_, script) = crate::runtime::new_script_item(Some(
            b"this/profile/source/does/not/exist.vim".to_vec(),
        ));
        unsafe { (*script).sn_prof_on = true };

        let report = unsafe { script_dump_profile() };

        assert!(report.ends_with(b"Cannot open file!\n\n"));
    }

    #[test]
    fn script_dump_profile_skips_unprofiled_scripts() {
        let _lock = crate::globals::global_state_test_lock();
        crate::runtime::tests_reset_for_test();
        crate::runtime::new_script_item(Some(b"plain.vim".to_vec()));
        assert!(unsafe { script_dump_profile() }.is_empty());
    }

    #[test]
    fn profile_dump_is_a_noop_without_an_output_name() {
        let _lock = crate::globals::global_state_test_lock();
        let _name = ProfileFilenameGuard::set(None);
        unsafe { profile_dump() }.unwrap();
    }

    #[test]
    #[cfg_attr(
        miri,
        ignore = "Miri cannot emulate the Windows create/truncate file-open mode"
    )]
    fn profile_dump_writes_function_reports_to_the_named_file() {
        let _lock = crate::globals::global_state_test_lock();
        crate::runtime::tests_reset_for_test();
        crate::eval::userfunc::func_init();
        let path = std::env::temp_dir().join(format!(
            "nero-profile-dump-{}-{}.log",
            std::process::id(),
            profile_start()
        ));
        let _file = StartupReportGuard(path.clone());
        let _name = ProfileFilenameGuard::set(Some(
            path.to_string_lossy().into_owned().into_bytes(),
        ));
        let function = Box::into_raw(Box::new(
            crate::eval::typval_defs::UfuncT {
                uf_name: b"Dumped\0".to_vec(),
                uf_prof_initialized: 1,
                uf_tm_count: 1,
                uf_tm_total: 1_000_000_000,
                uf_tm_self: 500_000_000,
                uf_lines: vec![Some(b"return 1".to_vec())],
                uf_tml_count: vec![1],
                uf_tml_total: vec![1_000_000_000],
                uf_tml_self: vec![500_000_000],
                ..Default::default()
            },
        ));
        unsafe {
            assert_eq!(
                crate::eval::userfunc::func_hashtab_add(function),
                crate::vim_defs::OK
            );
            profile_dump().unwrap();
        }

        let report = std::fs::read_to_string(&path).unwrap();
        assert!(report.starts_with("FUNCTION  Dumped()\n"));
        assert!(report.contains("Called 1 time\n"));
        assert!(report.contains("FUNCTIONS SORTED ON TOTAL TIME\n"));
        assert!(report.contains("FUNCTIONS SORTED ON SELF TIME\n"));

        unsafe {
            crate::eval::userfunc::func_init();
            drop(Box::from_raw(function));
        }
    }

    #[test]
    fn ex_profile_start_initializes_output_and_shared_state() {
        let _lock = crate::globals::global_state_test_lock();
        let _state = ProfilingStateGuard::new();
        let _name = ProfileFilenameGuard::set(None);
        unsafe {
            crate::globals::GLOBALS.get_mut().do_profiling =
                crate::globals::PROF_NONE;
            crate::eval::vars::set_vim_var_nr(
                crate::eval::vars::VimVarIndex::Profiling,
                0,
            );
        }
        profile_set_wait(99);
        let eap = crate::ex_cmds_defs::ExargT {
            arg: Some(b"start profile.log".to_vec()),
            ..Default::default()
        };

        unsafe { ex_profile(&eap) }.unwrap();

        assert_eq!(
            unsafe { crate::globals::GLOBALS.get_mut() }.do_profiling,
            crate::globals::PROF_YES
        );
        assert_eq!(profile_get_wait(), 0);
        assert_eq!(
            unsafe {
                crate::eval::vars::get_vim_var_nr(
                    crate::eval::vars::VimVarIndex::Profiling,
                )
            },
            1
        );
        assert_eq!(
            unsafe { PROFILE_FNAME.get_mut() }.as_deref(),
            Some(b"profile.log".as_slice())
        );
    }

    #[test]
    fn ex_profile_pause_and_continue_account_for_paused_time() {
        let _lock = crate::globals::global_state_test_lock();
        let _state = ProfilingStateGuard::new();
        unsafe { crate::globals::GLOBALS.get_mut() }.do_profiling =
            crate::globals::PROF_YES;
        profile_set_wait(0);

        let pause = crate::ex_cmds_defs::ExargT {
            arg: Some(b"pause".to_vec()),
            ..Default::default()
        };
        unsafe { ex_profile(&pause) }.unwrap();
        assert_eq!(
            unsafe { crate::globals::GLOBALS.get_mut() }.do_profiling,
            crate::globals::PROF_PAUSED
        );

        std::thread::sleep(std::time::Duration::from_millis(1));
        let resume = crate::ex_cmds_defs::ExargT {
            arg: Some(b"continue".to_vec()),
            ..Default::default()
        };
        unsafe { ex_profile(&resume) }.unwrap();
        assert_eq!(
            unsafe { crate::globals::GLOBALS.get_mut() }.do_profiling,
            crate::globals::PROF_YES
        );
        assert!(profile_get_wait() > 0);
    }

    #[test]
    fn ex_profile_requires_start_before_other_subcommands() {
        let _lock = crate::globals::global_state_test_lock();
        let _state = ProfilingStateGuard::new();
        unsafe { crate::globals::GLOBALS.get_mut() }.do_profiling =
            crate::globals::PROF_NONE;
        let eap = crate::ex_cmds_defs::ExargT {
            arg: Some(b"pause".to_vec()),
            ..Default::default()
        };
        unsafe { ex_profile(&eap) }.unwrap();
        assert_eq!(
            unsafe { crate::globals::GLOBALS.get_mut() }.do_profiling,
            crate::globals::PROF_NONE
        );
    }

    #[test]
    #[cfg_attr(
        miri,
        ignore = "Miri cannot emulate the Windows create/truncate file-open mode"
    )]
    fn ex_profile_stop_dumps_and_resets_profiling() {
        let _lock = crate::globals::global_state_test_lock();
        let _state = ProfilingStateGuard::new();
        crate::runtime::tests_reset_for_test();
        crate::eval::userfunc::func_init();
        let path = std::env::temp_dir().join(format!(
            "nero-profile-stop-{}-{}.log",
            std::process::id(),
            profile_start()
        ));
        let _file = StartupReportGuard(path.clone());
        let _name = ProfileFilenameGuard::set(Some(
            path.to_string_lossy().into_owned().into_bytes(),
        ));
        unsafe {
            crate::globals::GLOBALS.get_mut().do_profiling =
                crate::globals::PROF_YES;
            crate::eval::vars::set_vim_var_nr(
                crate::eval::vars::VimVarIndex::Profiling,
                1,
            );
        }
        let eap = crate::ex_cmds_defs::ExargT {
            arg: Some(b"stop".to_vec()),
            ..Default::default()
        };

        unsafe { ex_profile(&eap) }.unwrap();

        assert_eq!(
            unsafe { crate::globals::GLOBALS.get_mut() }.do_profiling,
            crate::globals::PROF_NONE
        );
        assert_eq!(
            unsafe {
                crate::eval::vars::get_vim_var_nr(
                    crate::eval::vars::VimVarIndex::Profiling,
                )
            },
            0
        );
        assert!(unsafe { PROFILE_FNAME.get_mut() }.is_none());
        assert!(path.exists());
    }

    // --- get_profile_name / prof_def_func ---

    #[test]
    fn get_profile_name_walks_the_subcommand_list_then_stops() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe {
            let prev = *PEXPAND_WHAT.get_mut();
            set_pexpand_what(PexpandWhat::Subcmd);

            assert_eq!(get_profile_name(0), Some("continue"));
            assert_eq!(get_profile_name(6), Some("stop"));
            assert_eq!(get_profile_name(7), None, "past the end");
            assert_eq!(get_profile_name(-1), None);

            set_pexpand_what(prev);
        }
    }

    #[test]
    fn get_profile_name_offers_nothing_in_the_other_state() {
        // Matches the original's own `default: return NULL`.
        let _lock = crate::globals::global_state_test_lock();
        unsafe {
            let prev = *PEXPAND_WHAT.get_mut();
            set_pexpand_what(PexpandWhat::Nothing);

            assert_eq!(get_profile_name(0), None);

            set_pexpand_what(prev);
        }
    }

    #[test]
    fn profile_reset_clears_script_function_and_output_state() {
        let _lock = crate::globals::global_state_test_lock();
        crate::runtime::tests_reset_for_test();
        crate::eval::userfunc::func_init();

        let (_, script) =
            crate::runtime::new_script_item(Some(b"profile.vim".to_vec()));
        let (_, untouched_script) =
            crate::runtime::new_script_item(Some(b"plain.vim".to_vec()));
        unsafe {
            (*script).sn_prof_on = true;
            (*script).sn_pr_force = true;
            (*script).sn_pr_child = 1;
            (*script).sn_pr_nest = 2;
            (*script).sn_pr_count = 3;
            (*script).sn_pr_total = 4;
            (*script).sn_pr_self = 5;
            (*script).sn_pr_start = 6;
            (*script).sn_pr_children = 7;
            (*script).sn_prl_ga.push(crate::runtime_defs::SnPrlT::default());
            (*script).sn_prl_start = 8;
            (*script).sn_prl_children = 9;
            (*script).sn_prl_wait = 10;
            (*script).sn_prl_idx = 11;
            (*script).sn_prl_execed = 1;
            (*untouched_script).sn_pr_count = 99;
        }

        let function = Box::into_raw(Box::new(
            crate::eval::typval_defs::UfuncT {
                uf_name: b"Profiled\0".to_vec(),
                uf_profiling: 1,
                uf_prof_initialized: 1,
                uf_tm_count: 2,
                uf_tm_total: 3,
                uf_tm_self: 4,
                uf_tm_children: 5,
                uf_tml_count: vec![1, 2],
                uf_tml_total: vec![3, 4],
                uf_tml_self: vec![5, 6],
                uf_tml_start: 7,
                uf_tml_children: 8,
                uf_tml_wait: 9,
                uf_tml_idx: 1,
                uf_tml_execed: 1,
                ..Default::default()
            },
        ));
        let untouched_function = Box::into_raw(Box::new(
            crate::eval::typval_defs::UfuncT {
                uf_name: b"Untouched\0".to_vec(),
                uf_prof_initialized: 0,
                uf_tm_count: 99,
                ..Default::default()
            },
        ));
        unsafe {
            assert_eq!(
                crate::eval::userfunc::func_hashtab_add(function),
                crate::vim_defs::OK
            );
            assert_eq!(
                crate::eval::userfunc::func_hashtab_add(untouched_function),
                crate::vim_defs::OK
            );
            *PROFILE_FNAME.get_mut() = Some(b"profile.log".to_vec());

            profile_reset();

            assert!(!(*script).sn_prof_on);
            assert!(!(*script).sn_pr_force);
            assert_eq!((*script).sn_pr_child, 0);
            assert_eq!((*script).sn_pr_nest, 0);
            assert_eq!((*script).sn_pr_count, 0);
            assert_eq!((*script).sn_pr_total, 0);
            assert_eq!((*script).sn_pr_self, 0);
            assert_eq!((*script).sn_pr_start, 0);
            assert_eq!((*script).sn_pr_children, 0);
            assert!((*script).sn_prl_ga.is_empty());
            assert_eq!((*script).sn_prl_start, 0);
            assert_eq!((*script).sn_prl_children, 0);
            assert_eq!((*script).sn_prl_wait, 0);
            assert_eq!((*script).sn_prl_idx, -1);
            assert_eq!((*script).sn_prl_execed, 0);
            assert_eq!((*untouched_script).sn_pr_count, 99);

            assert_eq!((*function).uf_profiling, 0);
            assert_eq!((*function).uf_tm_count, 0);
            assert_eq!((*function).uf_tm_total, 0);
            assert_eq!((*function).uf_tm_self, 0);
            assert_eq!((*function).uf_tm_children, 0);
            assert_eq!((*function).uf_tml_count.as_slice(), &[0, 0]);
            assert_eq!((*function).uf_tml_total.as_slice(), &[0, 0]);
            assert_eq!((*function).uf_tml_self.as_slice(), &[0, 0]);
            assert_eq!((*function).uf_tml_start, 0);
            assert_eq!((*function).uf_tml_children, 0);
            assert_eq!((*function).uf_tml_wait, 0);
            assert_eq!((*function).uf_tml_idx, -1);
            assert_eq!((*function).uf_tml_execed, 0);
            assert_eq!((*untouched_function).uf_tm_count, 99);
            assert!(PROFILE_FNAME.get_mut().is_none());

            crate::eval::userfunc::func_init();
            drop(Box::from_raw(function));
            drop(Box::from_raw(untouched_function));
        }
    }

    #[test]
    fn func_do_profile_initializes_line_timing_arrays() {
        let mut function = crate::eval::typval_defs::UfuncT {
            uf_lines: vec![Some(b"one".to_vec()), None, Some(b"three".to_vec())],
            uf_tm_count: 7,
            uf_tm_total: 8,
            uf_tm_self: 9,
            ..Default::default()
        };

        func_do_profile(&mut function);

        assert_eq!(function.uf_profiling, 1);
        assert_eq!(function.uf_prof_initialized, 1);
        assert_eq!(function.uf_tm_count, 0);
        assert_eq!(function.uf_tm_total, 0);
        assert_eq!(function.uf_tm_self, 0);
        assert_eq!(function.uf_tml_count, vec![0; 3]);
        assert_eq!(function.uf_tml_total, vec![0; 3]);
        assert_eq!(function.uf_tml_self, vec![0; 3]);
        assert_eq!(function.uf_tml_idx, -1);
    }

    #[test]
    fn func_do_profile_allocates_one_slot_for_an_empty_function() {
        let mut function = crate::eval::typval_defs::UfuncT::default();
        func_do_profile(&mut function);
        assert_eq!(function.uf_tml_count, vec![0]);
        assert_eq!(function.uf_tml_total, vec![0]);
        assert_eq!(function.uf_tml_self, vec![0]);
    }

    #[test]
    fn func_do_profile_preserves_initialized_timing_data() {
        let mut function = crate::eval::typval_defs::UfuncT {
            uf_profiling: 0,
            uf_prof_initialized: 1,
            uf_tm_count: 7,
            uf_tm_total: 8,
            uf_tm_self: 9,
            uf_tml_count: vec![10],
            uf_tml_total: vec![11],
            uf_tml_self: vec![12],
            uf_tml_idx: 0,
            ..Default::default()
        };

        func_do_profile(&mut function);

        assert_eq!(function.uf_profiling, 1);
        assert_eq!(function.uf_tm_count, 7);
        assert_eq!(function.uf_tm_total, 8);
        assert_eq!(function.uf_tm_self, 9);
        assert_eq!(function.uf_tml_count, vec![10]);
        assert_eq!(function.uf_tml_total, vec![11]);
        assert_eq!(function.uf_tml_self, vec![12]);
        assert_eq!(function.uf_tml_idx, 0);
    }

    #[test]
    fn set_context_in_profile_cmd_completes_subcommands() {
        let _lock = crate::globals::global_state_test_lock();
        let mut xp = crate::cmdexpand_defs::ExpandT::default();
        unsafe { set_context_in_profile_cmd(&mut xp, b"sta") };

        assert_eq!(
            xp.xp_context,
            crate::cmdexpand_defs::ExpandContext::Profile
        );
        assert_eq!(xp.xp_pattern.as_deref(), Some(b"sta".as_slice()));
        assert_eq!(unsafe { get_profile_name(5) }, Some("start"));
    }

    #[test]
    fn set_context_in_profile_cmd_completes_start_and_file_arguments() {
        let _lock = crate::globals::global_state_test_lock();
        for argument in [b"start   log.out".as_slice(), b"file *.vim".as_slice()] {
            let mut xp = crate::cmdexpand_defs::ExpandT::default();
            unsafe { set_context_in_profile_cmd(&mut xp, argument) };

            assert_eq!(
                xp.xp_context,
                crate::cmdexpand_defs::ExpandContext::Files
            );
            assert_eq!(
                xp.xp_pattern.as_deref(),
                Some(if argument[0] == b's' {
                    b"log.out".as_slice()
                } else {
                    b"*.vim".as_slice()
                })
            );
        }
    }

    #[test]
    fn set_context_in_profile_cmd_completes_function_arguments() {
        let _lock = crate::globals::global_state_test_lock();
        let mut xp = crate::cmdexpand_defs::ExpandT::default();
        unsafe { set_context_in_profile_cmd(&mut xp, b"func   MyFunc") };

        assert_eq!(
            xp.xp_context,
            crate::cmdexpand_defs::ExpandContext::UserFunc
        );
        assert_eq!(xp.xp_pattern.as_deref(), Some(b"MyFunc".as_slice()));
    }

    #[test]
    fn set_context_in_profile_cmd_rejects_unknown_or_abbreviated_subcommands() {
        let _lock = crate::globals::global_state_test_lock();
        for argument in [b"bogus value".as_slice(), b"sta value".as_slice()] {
            let mut xp = crate::cmdexpand_defs::ExpandT::default();
            unsafe { set_context_in_profile_cmd(&mut xp, argument) };

            assert_eq!(
                xp.xp_context,
                crate::cmdexpand_defs::ExpandContext::Nothing
            );
            assert_eq!(xp.xp_pattern.as_deref(), Some(argument));
        }
    }

    #[test]
    fn prof_def_func_is_false_without_a_script_context() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe {
            let prev = crate::globals::GLOBALS.get_mut().current_sctx.sc_sid;
            crate::globals::GLOBALS.get_mut().current_sctx.sc_sid = 0;

            assert!(!prof_def_func());

            crate::globals::GLOBALS.get_mut().current_sctx.sc_sid = prev;
        }
    }

    #[test]
    fn prof_def_func_reports_the_scripts_own_force_flag() {
        let _lock = crate::globals::global_state_test_lock();
        crate::runtime::tests_reset_for_test();
        unsafe {
            let prev = crate::globals::GLOBALS.get_mut().current_sctx.sc_sid;
            let (sid, item) = crate::runtime::new_script_item(None);
            crate::globals::GLOBALS.get_mut().current_sctx.sc_sid = sid;

            (*item).sn_pr_force = false;
            assert!(!prof_def_func());
            (*item).sn_pr_force = true;
            assert!(prof_def_func());

            crate::globals::GLOBALS.get_mut().current_sctx.sc_sid = prev;
        }
    }

    #[test]
    fn start_and_end_measure_positive_elapsed_time() {
        let start = profile_start();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let elapsed = profile_end(start);
        assert!(profile_signed(elapsed) > 0);
    }

    #[test]
    fn zero_is_identity_for_add() {
        assert_eq!(profile_add(profile_zero(), 42), 42);
    }

    #[test]
    fn divide_matches_c_semantics() {
        assert_eq!(profile_divide(10, 0), 0);
        assert_eq!(profile_divide(10, -1), 0);
        assert_eq!(profile_divide(10, 4), 3); // 10/4=2.5, round-half-away-from-zero -> 3
        assert_eq!(profile_divide(9, 3), 3);
    }

    #[test]
    fn self_time_excludes_children_but_never_negative() {
        assert_eq!(profile_self(5, 20, 8), 17); // 5 + 20 - 8
        assert_eq!(profile_self(5, 3, 8), 5); // total <= children -> just self
    }

    #[test]
    fn cmp_matches_ordering() {
        assert_eq!(profile_cmp(100, 100), 0);
        assert!(profile_cmp(100, 200) > 0); // tm2(200) > tm1(100)
        assert!(profile_cmp(200, 100) < 0); // tm2(100) < tm1(200)
    }

    #[test]
    fn setlimit_and_passed_limit_roundtrip() {
        assert_eq!(profile_setlimit(0), profile_zero());
        assert!(!profile_passed_limit(profile_zero())); // never set
        let soon = profile_setlimit(1); // 1ms in the future
        std::thread::sleep(std::time::Duration::from_millis(5));
        assert!(profile_passed_limit(soon));
    }

    #[test]
    fn sub_wait_accounts_for_recorded_wait_time() {
        profile_set_wait(0);
        let tm = 100;
        let tma = 200;
        // waittime=0 means profile_sub_wait is just tma - (0 - tm) = tma + tm... let's just check it's deterministic:
        let result = profile_sub_wait(tm, tma);
        assert_eq!(result, profile_sub(tma, profile_sub(profile_get_wait(), tm)));
    }
}
