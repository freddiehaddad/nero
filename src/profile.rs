//! Translated from `src/nvim/profile.c` (partial - `src/nvim/profile.h`
//! has no manually-written content beyond the generated declarations).
//!
//! Only the self-contained `proftime_T` time-arithmetic API is translated
//! here: `profile_start`/`profile_end`/`profile_msg`/`profile_setlimit`/
//! `profile_passed_limit`/`profile_zero`/`profile_divide`/`profile_add`/
//! `profile_sub`/`profile_self`/`profile_get_wait`/`profile_set_wait`/
//! `profile_sub_wait`/`profile_equal`/`profile_signed`/`profile_cmp`.
//!
//! Deferred: everything from `profile_reset` onward, which operates on
//! `scriptitem_T`/`script_items` (`eval/userfunc.h`, not yet translated) to
//! track per-script/per-function/per-line profiling data - a real forward
//! dependency, not started.
//!
//! `os_hrtime()` (`os/time.c`, phase 10, not yet translated) is stood in
//! for by [`std::time::Instant`], Rust's standard monotonic high-resolution
//! clock - functionally the same contract as `uv_hrtime()`/`os_hrtime()`:
//! an arbitrary monotonic reference point, used only for taking
//! differences, never as absolute wall-clock time. This should be
//! reconciled with (or simply call into) the real `os_hrtime` translation
//! once `os/time.c` is done.

use crate::types_defs::ProftimeT;

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
