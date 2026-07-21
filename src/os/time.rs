//! Translated from `src/nvim/os/time.c` (tractable core only).
//!
//! Translated: `os_time` (`Timestamp`, seconds since the Unix epoch),
//! `os_realtime` (nanoseconds since the Unix epoch), `os_hrtime`
//! (monotonically-increasing nanosecond counter relative to an
//! arbitrary point in the past), and `os_sleep` - all four are, in the
//! original, thin wrappers around either the C standard library
//! (`time()`) or libuv (`uv_hrtime`/`uv_clock_gettime`/`uv_sleep`)
//! purely for portability; Rust's own `std::time` already provides the
//! exact same portable primitives directly, with no need for libuv or
//! an event loop, so these four are translated now rather than waiting
//! for the still-open libuv FFI-vs-Rust-runtime decision (phase 11).
//!
//! Deferred (each needs a not-yet-translated subsystem):
//! - `os_now`: needs `main_loop.uv`'s cached loop time (the event loop,
//!   phase 11 - unlike the four functions above, this one's whole
//!   contract is "the loop's cached time", not a fresh OS query).
//! - `os_delay`: needs `LOOP_PROCESS_EVENTS_UNTIL` (the event loop,
//!   phase 11) and `os_input_ready` (`os/input.c`).
//! - `os_localtime_r`/`os_localtime`/`os_ctime_r`/`os_ctime`: need a
//!   portable local-timezone conversion. Rust's std has no built-in
//!   equivalent to POSIX `localtime_r`/Windows `localtime` (unlike the
//!   four functions above, this genuinely needs either raw libc/Win32
//!   FFI or a new crate dependency like `time`/`chrono` - a real,
//!   undecided scope question, not a trivial std-library swap).
//! - `os_strptime`: needs POSIX `strptime` (unavailable on Windows in
//!   the original too - `HAVE_STRPTIME`-gated) or an equivalent parser;
//!   same "needs a real dependency decision" blocker as the above.

use super::time_defs::Timestamp;
use std::sync::LazyLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// An arbitrary fixed point in the past (process start), used as the
/// zero-point for [`os_hrtime`] - matches the original's contract that
/// `os_hrtime`'s epoch is arbitrary and only differences between calls
/// are meaningful.
static HRTIME_EPOCH: LazyLock<Instant> = LazyLock::new(Instant::now);

/// Obtains the current Unix timestamp (`os_time`).
///
/// @return Seconds since epoch.
#[must_use]
pub fn os_time() -> Timestamp {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Obtains the current system time from a high-resolution real-time
/// clock source (`os_realtime`).
///
/// The real-time clock counts from the UNIX epoch (1970-01-01) and is
/// subject to time adjustments; it can jump back in time.
///
/// @return Nanoseconds since epoch or 0.
#[must_use]
pub fn os_realtime() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0)
}

/// Gets a high-resolution (nanosecond), monotonically-increasing time
/// relative to an arbitrary time in the past (`os_hrtime`).
///
/// Not related to the time of day and therefore not subject to clock
/// drift.
///
/// @return Relative time value with nanosecond precision.
#[must_use]
pub fn os_hrtime() -> u64 {
    Instant::now().duration_since(*HRTIME_EPOCH).as_nanos() as u64
}

/// Sleeps for `ms` milliseconds without checking for events or
/// interrupts (`os_sleep`).
///
/// This blocks even "fast" events which is quite disruptive. This
/// should only be used in debug code. Prefer `os_delay` (not yet
/// translated - needs the event loop) and decide if the delay should be
/// interrupted by input or only a CTRL-C.
pub fn os_sleep(ms: u64) {
    let ms = ms.min(u32::MAX as u64);
    std::thread::sleep(std::time::Duration::from_millis(ms));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_time_is_a_plausible_unix_timestamp() {
        // Any time after 2024-01-01T00:00:00Z (1704067200) and before a
        // generous upper bound, so this doesn't assume a specific clock
        // but still catches a badly broken implementation.
        let t = os_time();
        assert!(t > 1_704_067_200);
        assert!(t < 4_102_444_800); // 2100-01-01T00:00:00Z
    }

    #[test]
    fn os_realtime_is_consistent_with_os_time() {
        let secs_from_realtime = os_realtime() / 1_000_000_000;
        let t = os_time() as i64;
        // Both read "now" independently, allow a little slack.
        assert!((secs_from_realtime - t).abs() <= 2);
    }

    #[test]
    fn os_hrtime_is_monotonically_increasing() {
        let a = os_hrtime();
        std::thread::sleep(std::time::Duration::from_millis(5));
        let b = os_hrtime();
        assert!(b > a);
    }

    #[test]
    fn os_sleep_sleeps_for_at_least_the_requested_duration() {
        let start = Instant::now();
        os_sleep(10);
        assert!(start.elapsed() >= std::time::Duration::from_millis(10));
    }
}
