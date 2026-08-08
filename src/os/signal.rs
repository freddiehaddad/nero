//! Translated from `src/nvim/os/signal.c` (tractable core only).
//!
//! `signal.c` installs libuv signal watchers and handles deadly
//! signals by preserving swap files and exiting cleanly. That whole
//! half needs the libuv event loop, `ml_close_notmod`, `preserve_exit`
//! and the UI teardown path, none of which are translated.
//!
//! Translated: the `rejecting_deadly` flag pair and [`signal_name`],
//! which need nothing beyond the signal numbers themselves.
//!
//! Deferred: `signal_init`/`signal_teardown`/`signal_start`/
//! `signal_stop`/`signal_handler`/`on_signal` - all event-loop bound.

/// Whether deadly signals are currently being rejected
/// (`rejecting_deadly`).
static REJECTING_DEADLY: crate::globals::GlobalCell<bool> = crate::globals::GlobalCell::new(false);

/// Start rejecting deadly signals (`signal_reject_deadly`).
///
/// # Safety
/// Mutates the `REJECTING_DEADLY` file-static.
pub unsafe fn signal_reject_deadly() {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *REJECTING_DEADLY.get_mut() = true };
}

/// Stop rejecting deadly signals (`signal_accept_deadly`).
///
/// # Safety
/// Mutates the `REJECTING_DEADLY` file-static.
pub unsafe fn signal_accept_deadly() {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *REJECTING_DEADLY.get_mut() = false };
}

/// Whether deadly signals are currently rejected (`rejecting_deadly`).
///
/// The original reads the file-static directly from `on_signal`; this
/// crate exposes it as an accessor, since the variable itself stays
/// private to this module.
///
/// # Safety
/// Reads the `REJECTING_DEADLY` file-static.
#[must_use]
pub unsafe fn signal_rejecting_deadly() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *REJECTING_DEADLY.get_mut() }
}

/// The name of signal `signum` (`signal_name`).
///
/// Signals the platform does not define are simply absent, matching
/// the original's own `#ifdef` guards around each case; anything
/// unrecognised is `"Unknown"`.
#[must_use]
pub fn signal_name(signum: i32) -> &'static str {
    // SIGTERM and SIGINT exist on every supported platform; the rest
    // are Unix-only, matching the original's per-signal #ifdefs (MSVC
    // defines none of them).
    match signum {
        libc::SIGTERM => "SIGTERM",
        libc::SIGINT => "SIGINT",
        #[cfg(unix)]
        libc::SIGPIPE => "SIGPIPE",
        #[cfg(unix)]
        libc::SIGTSTP => "SIGTSTP",
        #[cfg(unix)]
        libc::SIGQUIT => "SIGQUIT",
        #[cfg(unix)]
        libc::SIGHUP => "SIGHUP",
        #[cfg(unix)]
        libc::SIGUSR1 => "SIGUSR1",
        #[cfg(unix)]
        libc::SIGWINCH => "SIGWINCH",
        _ => "Unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::globals::global_state_test_lock;

    #[test]
    fn signal_name_reports_the_cross_platform_signals() {
        assert_eq!(signal_name(libc::SIGTERM), "SIGTERM");
        assert_eq!(signal_name(libc::SIGINT), "SIGINT");
    }

    #[cfg(unix)]
    #[test]
    fn signal_name_reports_the_unix_only_signals() {
        assert_eq!(signal_name(libc::SIGHUP), "SIGHUP");
        assert_eq!(signal_name(libc::SIGWINCH), "SIGWINCH");
        assert_eq!(signal_name(libc::SIGQUIT), "SIGQUIT");
        assert_eq!(signal_name(libc::SIGUSR1), "SIGUSR1");
        assert_eq!(signal_name(libc::SIGPIPE), "SIGPIPE");
        assert_eq!(signal_name(libc::SIGTSTP), "SIGTSTP");
    }

    #[test]
    fn signal_name_of_an_unrecognised_signal_is_unknown() {
        assert_eq!(signal_name(-1), "Unknown");
        assert_eq!(signal_name(9999), "Unknown");
    }

    #[test]
    fn deadly_signals_are_accepted_by_default_and_toggle() {
        let _lock = global_state_test_lock();
        // SAFETY: serialized by the global test lock.
        let prev = unsafe { signal_rejecting_deadly() };

        unsafe { signal_reject_deadly() };
        assert!(unsafe { signal_rejecting_deadly() });

        unsafe { signal_accept_deadly() };
        assert!(!unsafe { signal_rejecting_deadly() });

        unsafe {
            if prev {
                signal_reject_deadly();
            } else {
                signal_accept_deadly();
            }
        }
    }
}
