//! Translated from `src/nvim/os/proc.c` (tractable core only).
//!
//! Translated: `os_proc_running`.
//!
//! Deferred: `os_proc_tree_kill_rec`/`os_proc_tree_kill`/
//! `os_proc_children`/`os_proc_info` - process tree enumeration needs
//! platform-specific process-listing APIs (`/proc` on Linux,
//! `Toolhelp32Snapshot` on Windows) well beyond a single-PID liveness
//! check, out of scope for this pass.

/// Checks whether the process with the given `pid` is currently
/// running (`os_proc_running`).
///
/// The original is:
/// ```c
/// bool os_proc_running(int pid) {
///   int err = uv_kill(pid, 0);
///   if (err == 0) { return true; }
///   if (err == UV_ESRCH) { return false; }
///   return true;  // EPERM or anything else: assume still running.
/// }
/// ```
/// i.e. libuv's `uv_kill(pid, 0)` (a "signal 0" no-op existence check),
/// trichotomized into "definitely not running" (`ESRCH` only) vs.
/// "running or indeterminate" (everything else, including permission
/// errors) - deliberately erring towards `true`.
///
/// Unix: `uv_kill` reduces to plain `kill(pid, 0)`; translated via
/// `libc::kill` (already a dependency of this crate, used elsewhere for
/// locale functions).
///
/// Windows: libuv's `uv_kill`/`uv__kill` (`src/win/process.c`) do NOT
/// simply wrap one API call - verified against the real upstream source
/// rather than assumed. Step by step: `pid == 0` is special-cased to
/// `GetCurrentProcess()` (a pseudo-handle for the caller itself) -
/// Windows has no "process 0" and no POSIX-style process-group
/// broadcast semantics for pid 0, a genuine platform divergence
/// preserved here rather than papered over. Otherwise,
/// `OpenProcess(PROCESS_TERMINATE | PROCESS_QUERY_INFORMATION |
/// SYNCHRONIZE, FALSE, pid)` is used - note libuv requests
/// `PROCESS_TERMINATE` too (needed for its general kill-signal path,
/// unused by the signum=0 health check this function implements),
/// matched exactly anyway so a permission failure behaves identically
/// to the original in every case, not just the subset this function
/// exercises. If `OpenProcess` fails, `ERROR_INVALID_PARAMETER`
/// specifically means "no such process" (`ESRCH` -> `false`); any other
/// failure (e.g. access denied on someone else's process) is "not
/// ESRCH" -> assumed running (`true`), matching how
/// `uv_translate_sys_error`'s win32-code table never produces
/// `UV_ESRCH` on its own. Otherwise (`uv__kill`'s `signum == 0`
/// health-check branch), `GetExitCodeProcess` is queried first;
/// `status != STILL_ACTIVE` means the process has exited (`ESRCH` ->
/// `false`). If still `STILL_ACTIVE`, a zero-timeout
/// `WaitForSingleObject` is used as a race-condition safety net (a
/// process that itself happens to exit with code 259 would otherwise
/// look falsely alive): `WAIT_OBJECT_0` (signaled, i.e. exited) ->
/// `false`; `WAIT_TIMEOUT` (not signaled, still running) -> `true`;
/// anything else (`WAIT_FAILED` or unexpected) -> assumed running
/// (`true`), matching the "never ESRCH from here" default.
///
/// Implemented via hand-written Win32 FFI (no new crate dependency,
/// matching this crate's existing use of `libc` for direct system-API
/// FFI on Unix). An earlier draft used only `OpenProcess`+
/// `WaitForSingleObject` (omitting `SYNCHRONIZE` from the access mask
/// and `GetExitCodeProcess` entirely) - a standalone `rustc` scratch
/// test against this real machine caught both a hard failure
/// (`WaitForSingleObject` returning `WAIT_FAILED` without
/// `SYNCHRONIZE`) and, on fixing that, still 2 real algorithmic
/// differences from upstream, which is why this now matches the actual
/// libuv source function-for-function instead.
#[must_use]
pub fn os_proc_running(pid: i32) -> bool {
    #[cfg(unix)]
    {
        os_proc_running_unix(pid)
    }
    #[cfg(windows)]
    {
        os_proc_running_windows(pid)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        false
    }
}

#[cfg(unix)]
fn os_proc_running_unix(pid: i32) -> bool {
    // SAFETY: kill(pid, 0) with signal 0 sends no signal; it only
    // validates that a process with this PID exists (and that we have
    // permission to signal it) - always safe to call with any pid
    // value, matching the original's own "signal 0" existence check.
    let ret = unsafe { libc::kill(pid, 0) };
    if ret == 0 {
        // If there is no error the process must be running.
        return true;
    }
    // If the error is ESRCH then the process is not running. If the
    // process is running and owned by another user we get EPERM. With
    // other errors the process might be running, assuming it is then.
    std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
}

#[cfg(windows)]
fn os_proc_running_windows(pid: i32) -> bool {
    // Hand-written Win32 FFI for the functions needed here - no new
    // crate dependency, matching this crate's existing use of `libc`
    // for direct system-API FFI on Unix. Mirrors libuv's `uv_kill`
    // (the `pid == 0` special case and `OpenProcess` access mask) and
    // `uv__kill`'s `signum == 0` branch (the `GetExitCodeProcess` +
    // `WaitForSingleObject`-fallback health check), verified against
    // `src/win/process.c` in the real libuv source.
    type Handle = *mut std::ffi::c_void;
    const PROCESS_TERMINATE: u32 = 0x0001;
    const PROCESS_QUERY_INFORMATION: u32 = 0x0400;
    const SYNCHRONIZE: u32 = 0x0010_0000;
    const STILL_ACTIVE: u32 = 259;
    const ERROR_INVALID_PARAMETER: u32 = 87;
    const WAIT_OBJECT_0: u32 = 0x0000_0000;
    const WAIT_TIMEOUT: u32 = 0x0000_0102;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> Handle;
        fn OpenProcess(dw_desired_access: u32, b_inherit_handle: i32, dw_process_id: u32)
            -> Handle;
        fn GetExitCodeProcess(h_process: Handle, lp_exit_code: *mut u32) -> i32;
        fn WaitForSingleObject(h_handle: Handle, dw_milliseconds: u32) -> u32;
        fn CloseHandle(h_object: Handle) -> i32;
        fn GetLastError() -> u32;
    }

    // uv_kill: pid 0 means "the current process" on Windows (there is
    // no POSIX-style process-group broadcast here) - GetCurrentProcess
    // returns a pseudo-handle that is always valid and needs no
    // permission check.
    let handle = if pid == 0 {
        // SAFETY: GetCurrentProcess takes no arguments and always
        // succeeds.
        unsafe { GetCurrentProcess() }
    } else {
        // SAFETY: plain FFI call with a fixed access-rights constant;
        // the returned handle, if non-null, is unconditionally closed
        // below before returning.
        unsafe {
            OpenProcess(
                PROCESS_TERMINATE | PROCESS_QUERY_INFORMATION | SYNCHRONIZE,
                0,
                pid as u32,
            )
        }
    };

    if handle.is_null() {
        // SAFETY: plain FFI call, no preconditions.
        let err = unsafe { GetLastError() };
        // ERROR_INVALID_PARAMETER means no such process (ESRCH); any
        // other failure (e.g. access denied) is treated as "some
        // other error", which os_proc_running maps to "assume
        // running".
        return err != ERROR_INVALID_PARAMETER;
    }

    let mut status: u32 = 0;
    // SAFETY: handle is a valid, just-obtained process handle (or the
    // GetCurrentProcess() pseudo-handle); status is a valid local
    // out-pointer.
    let got_exit_code = unsafe { GetExitCodeProcess(handle, &mut status) };
    let running = if got_exit_code == 0 {
        // GetExitCodeProcess itself failed. uv__kill would translate
        // the error and return it; os_proc_running treats any
        // non-ESRCH error as "assume running", and this win32 error
        // path never produces ESRCH (verified against libuv's own
        // uv_translate_sys_error table), so: assume running.
        true
    } else if status != STILL_ACTIVE {
        // The process has already exited.
        false
    } else {
        // Still STILL_ACTIVE: confirm with a zero-timeout wait, a
        // race-condition safety net for processes that themselves
        // exit with code 259 (which would otherwise look falsely
        // alive here) - matches uv__kill's own fallback exactly.
        // SAFETY: handle is valid and not shared with any other code.
        match unsafe { WaitForSingleObject(handle, 0) } {
            w if w == WAIT_OBJECT_0 => false, // signaled: has exited.
            w if w == WAIT_TIMEOUT => true,   // not signaled: still running.
            _ => true,                        // WAIT_FAILED/unexpected: assume running.
        }
    };
    // SAFETY: handle is valid and exclusively owned here; closed
    // exactly once, right before returning. Closing the
    // GetCurrentProcess() pseudo-handle is a documented Win32 no-op,
    // matching uv_kill's own unconditional CloseHandle call.
    unsafe { CloseHandle(handle) };
    running
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_proc_running_is_true_for_the_current_process() {
        assert!(os_proc_running(std::process::id() as i32));
    }

    #[test]
    fn os_proc_running_is_false_for_an_implausible_pid() {
        // Deliberately NOT testing pid == 0 in a platform-neutral way:
        // on Unix, kill(0, sig) broadcasts to the caller's whole
        // process group (POSIX), so kill(0, 0) would almost always
        // succeed (the caller is always in its own group) rather than
        // indicating "process 0 doesn't exist" - a real pitfall almost
        // baked into this test. Use an implausibly large PID instead,
        // comfortably beyond realistic PID ranges on both Unix
        // (pid_max is at most a few million) and Windows, so this
        // doesn't depend on any specific real PID being free at test
        // time. pid == 0's genuinely different, platform-specific
        // meaning is covered separately below.
        assert!(!os_proc_running(2_000_000_000));
    }

    #[cfg(windows)]
    #[test]
    fn os_proc_running_is_true_for_pid_zero_on_windows() {
        // Verified against the real libuv source (src/win/process.c):
        // uv_kill special-cases pid == 0 to GetCurrentProcess(), i.e.
        // "the calling process" - unlike POSIX, where pid 0 means
        // "broadcast to my process group" instead. So on Windows,
        // os_proc_running(0) reports the CALLER as running (true),
        // the opposite of what pid 0 would naively suggest.
        assert!(os_proc_running(0));
    }
}
