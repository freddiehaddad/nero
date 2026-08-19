//! Translated from `src/nvim/profile.c` (partial - `src/nvim/profile.h`
//! has no manually-written content beyond the generated declarations).
//!
//! Only the self-contained `proftime_T` time-arithmetic API is translated
//! here: `profile_start`/`profile_end`/`profile_msg`/`profile_setlimit`/
//! `profile_passed_limit`/`profile_zero`/`profile_divide`/`profile_add`/
//! `profile_sub`/`profile_self`/`profile_get_wait`/`profile_set_wait`/
//! `profile_sub_wait`/`profile_equal`/`profile_signed`/`profile_cmp`.
//!
//! Per-function/per-script profiling remains mostly deferred, but
//! [`func_line_exec`]/[`func_line_end`]/[`script_line_exec`] are now
//! translated because their function/script records have real fields.
//! [`script_prof_save`]/[`script_prof_restore`] record nested-child
//! timing state, and [`prof_child_enter`]/[`prof_child_exit`] perform
//! the paired function/script child measurement.
//! `:profile` subcommand completion is translated through
//! [`set_context_in_profile_cmd`]/[`get_profile_name`].
//!
//! `os_hrtime()` (`os/time.c`, phase 10, not yet translated) is stood in
//! for by [`std::time::Instant`], Rust's standard monotonic high-resolution
//! clock - functionally the same contract as `uv_hrtime()`/`os_hrtime()`:
//! an arbitrary monotonic reference point, used only for taking
//! differences, never as absolute wall-clock time. This should be
//! reconciled with (or simply call into) the real `os_hrtime` translation
//! once `os/time.c` is done.

use crate::types_defs::ProftimeT;

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

/// Stands in for `os_hrtime()` until `os/time.c` is translated - see module
/// docs. Returns nanoseconds since an arbitrary, fixed, monotonic
/// reference point established on first use.
fn os_hrtime_stub() -> ProftimeT {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    let start = *START.get_or_init(std::time::Instant::now);
    start.elapsed().as_nanos() as ProftimeT
}

/// Gets the current time (`profile_start`).
#[inline]
pub fn profile_start() -> ProftimeT {
    os_hrtime_stub()
}

/// Computes the time elapsed since `tm` (`profile_end`).
#[inline]
pub fn profile_end(tm: ProftimeT) -> ProftimeT {
    profile_sub(os_hrtime_stub(), tm)
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
    os_hrtime_stub().wrapping_add(nsec)
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
    profile_cmp(os_hrtime_stub(), tm) < 0
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
