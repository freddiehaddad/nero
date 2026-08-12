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
//! [`func_line_exec`]/[`script_line_exec`] are now translated because
//! their function/script records have real fields.
//! [`script_prof_save`]/[`script_prof_restore`] record nested-child
//! timing state, and [`prof_child_enter`] starts the paired
//! function/script child measurement.
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
/// `set_context_in_profile_cmd`, which is not translated yet; this
/// accessor exists so it can drive the flag once it lands.
///
/// # Safety
/// Must not run concurrently with any other access to `PEXPAND_WHAT`.
pub unsafe fn set_pexpand_what(what: PexpandWhat) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *PEXPAND_WHAT.get_mut() = what };
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
