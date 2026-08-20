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

use super::time_defs::Timestamp;
use std::sync::LazyLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// An arbitrary fixed point in the past (process start), used as the
/// zero-point for [`os_hrtime`] - matches the original's contract that
/// `os_hrtime`'s epoch is arbitrary and only differences between calls
/// are meaningful.
static HRTIME_EPOCH: LazyLock<Instant> = LazyLock::new(Instant::now);
static LOCALTIME_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(unix)]
static TZ_CACHE: std::sync::Mutex<Option<std::ffi::OsString>> =
    std::sync::Mutex::new(None);

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

/// Portable local-time conversion (`os_localtime_r`).
#[must_use]
pub fn os_localtime_r(clock: i64) -> Option<libc::tm> {
    let _lock = LOCALTIME_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    #[cfg(unix)]
    {
        let timezone = std::env::var_os("TZ");
        let mut cache = TZ_CACHE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if *cache != timezone {
            unsafe extern "C" {
                fn tzset();
            }
            unsafe { tzset() };
            *cache = timezone;
        }

        let raw = clock as libc::time_t;
        let mut result = std::mem::MaybeUninit::<libc::tm>::uninit();
        let converted =
            unsafe { libc::localtime_r(&raw, result.as_mut_ptr()) };
        if converted.is_null() {
            None
        } else {
            Some(unsafe { result.assume_init() })
        }
    }

    #[cfg(windows)]
    {
        #[link(name = "ucrt")]
        unsafe extern "C" {
            fn _localtime64_s(
                result: *mut libc::tm,
                clock: *const i64,
            ) -> i32;
        }
        let raw = clock;
        let mut result = std::mem::MaybeUninit::<libc::tm>::uninit();
        if unsafe { _localtime64_s(result.as_mut_ptr(), &raw) } != 0 {
            None
        } else {
            Some(unsafe { result.assume_init() })
        }
    }
}

/// Convert the current Unix timestamp to local time (`os_localtime`).
#[must_use]
pub fn os_localtime() -> Option<libc::tm> {
    os_localtime_r(os_time() as i64)
}

fn invalid_time_string(result_len: usize) -> Vec<u8> {
    let max = result_len.saturating_sub(2);
    b"(Invalid)"[..b"(Invalid)".len().min(max)].to_vec()
}

#[cfg(unix)]
unsafe fn system_strftime(
    buffer: *mut libc::c_char,
    size: usize,
    format: *const libc::c_char,
    time: *const libc::tm,
) -> usize {
    unsafe { libc::strftime(buffer, size, format, time) }
}

#[cfg(windows)]
unsafe fn system_strftime(
    buffer: *mut libc::c_char,
    size: usize,
    format: *const libc::c_char,
    time: *const libc::tm,
) -> usize {
    #[link(name = "ucrt")]
    unsafe extern "C" {
        fn strftime(
            buffer: *mut libc::c_char,
            size: usize,
            format: *const libc::c_char,
            time: *const libc::tm,
        ) -> usize;
    }
    unsafe { strftime(buffer, size, format, time) }
}

/// Format `clock` in local time (`os_ctime_r`).
#[must_use]
pub fn os_ctime_r(
    clock: i64,
    result_len: usize,
    add_newline: bool,
) -> Vec<u8> {
    if result_len < 2 {
        return Vec::new();
    }
    let mut output = if let Some(local) = os_localtime_r(clock) {
        let mut buffer = vec![0u8; result_len];
        let format = b"%a %b %d %H:%M:%S %Y\0";
        let len = unsafe {
            system_strftime(
                buffer.as_mut_ptr().cast(),
                result_len - 1,
                format.as_ptr().cast(),
                &local,
            )
        };
        if len == 0 {
            invalid_time_string(result_len)
        } else {
            buffer.truncate(len);
            buffer
        }
    } else {
        invalid_time_string(result_len)
    };
    if add_newline && output.len() + 1 < result_len {
        output.push(b'\n');
    }
    output
}

/// Format the current local time (`os_ctime`).
#[must_use]
pub fn os_ctime(result_len: usize, add_newline: bool) -> Vec<u8> {
    os_ctime_r(os_time() as i64, result_len, add_newline)
}

/// Format one local timestamp with a caller-provided `strftime`
/// format.
///
/// Returns `None` when local-time conversion fails and `Some(empty)`
/// when `strftime` cannot fit or interpret the result, matching
/// `f_strftime`'s two distinct outcomes.
#[must_use]
pub fn os_strftime(format: &[u8], clock: i64) -> Option<Vec<u8>> {
    let local = os_localtime_r(clock)?;
    let format_end =
        format.iter().position(|&byte| byte == 0).unwrap_or(format.len());
    let mut format_string = Vec::with_capacity(format_end + 1);
    format_string.extend_from_slice(&format[..format_end]);
    format_string.push(0);
    let mut result = [0u8; 256];
    let len = unsafe {
        system_strftime(
            result.as_mut_ptr().cast(),
            result.len(),
            format_string.as_ptr().cast(),
            &local,
        )
    };
    if len == 0 {
        Some(Vec::new())
    } else {
        Some(result[..len].to_vec())
    }
}

/// Parse local broken-down time using POSIX `strptime`
/// (`os_strptime`).
///
/// Returns `None` on Windows, matching the original's
/// `HAVE_STRPTIME`-gated fallback.
#[must_use]
pub fn os_strptime(input: &[u8], format: &[u8]) -> Option<libc::tm> {
    #[cfg(windows)]
    {
        let _ = (input, format);
        None
    }
    #[cfg(unix)]
    {
        let _lock = LOCALTIME_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let input_end =
            input.iter().position(|&byte| byte == 0).unwrap_or(input.len());
        let format_end = format
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(format.len());
        let input = std::ffi::CString::new(&input[..input_end]).ok()?;
        let format = std::ffi::CString::new(&format[..format_end]).ok()?;
        let mut parsed = std::mem::MaybeUninit::<libc::tm>::zeroed();
        unsafe { (*parsed.as_mut_ptr()).tm_isdst = -1 };
        let remainder = unsafe {
            libc::strptime(
                input.as_ptr(),
                format.as_ptr(),
                parsed.as_mut_ptr(),
            )
        };
        if remainder.is_null() {
            None
        } else {
            Some(unsafe { parsed.assume_init() })
        }
    }
}

#[cfg(unix)]
unsafe fn system_mktime(time: *mut libc::tm) -> i64 {
    unsafe { libc::mktime(time) as i64 }
}

#[cfg(windows)]
unsafe fn system_mktime(time: *mut libc::tm) -> i64 {
    #[link(name = "ucrt")]
    unsafe extern "C" {
        fn _mktime64(time: *mut libc::tm) -> i64;
    }
    unsafe { _mktime64(time) }
}

/// Convert local broken-down time to a Unix timestamp (`mktime`).
#[must_use]
pub fn os_mktime(time: &mut libc::tm) -> Option<i64> {
    let _lock = LOCALTIME_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let timestamp = unsafe { system_mktime(time) };
    (timestamp != -1).then_some(timestamp)
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

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call localtime_r/localtime")]
    fn os_localtime_returns_valid_calendar_fields() {
        let local = os_localtime().expect("current time converts");
        assert!((0..=11).contains(&local.tm_mon));
        assert!((1..=31).contains(&local.tm_mday));
        assert!((0..=23).contains(&local.tm_hour));
        assert!((0..=59).contains(&local.tm_min));
        assert!((0..=60).contains(&local.tm_sec));
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call localtime_r/localtime")]
    fn os_localtime_r_converts_the_unix_epoch() {
        let local = os_localtime_r(0).expect("epoch converts");
        assert!(matches!(local.tm_year, 69 | 70));
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call localtime/strftime FFI")]
    fn os_ctime_formats_current_time_and_optional_newline() {
        let without = os_ctime(128, false);
        assert!(!without.is_empty());
        assert!(!without.ends_with(b"\n"));

        let with = os_ctime(128, true);
        assert!(with.ends_with(b"\n"));
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call localtime/strftime FFI")]
    fn os_ctime_r_honors_small_buffer_bounds() {
        let formatted = os_ctime_r(0, 8, true);
        assert!(formatted.len() < 8);
        assert!(formatted == b"(Inval" || formatted == b"(Inval\n");
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call localtime/strftime FFI")]
    fn os_strftime_formats_year_and_honors_embedded_nul() {
        let year = os_strftime(b"%Y\0ignored", 0)
            .expect("epoch converts");
        assert!(year == b"1969" || year == b"1970");
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call localtime/strftime FFI")]
    fn os_strftime_returns_empty_when_output_exceeds_fixed_buffer() {
        let format = vec![b'x'; 300];
        assert_eq!(os_strftime(&format, 0), Some(Vec::new()));
    }

    #[cfg(unix)]
    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call strptime/mktime FFI")]
    fn os_strptime_and_mktime_parse_local_calendar_time() {
        let mut parsed = os_strptime(
            b"1970-01-02 03:04:05 trailing",
            b"%Y-%m-%d %H:%M:%S",
        )
        .expect("valid local time parses");
        assert_eq!(parsed.tm_year, 70);
        assert_eq!(parsed.tm_mon, 0);
        assert_eq!(parsed.tm_mday, 2);
        assert!(os_mktime(&mut parsed).is_some());
    }

    #[cfg(windows)]
    #[test]
    fn os_strptime_is_unavailable_on_windows() {
        assert!(os_strptime(b"1970", b"%Y").is_none());
    }
}
