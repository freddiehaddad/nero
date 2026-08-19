//! Translated from `src/nvim/os/proc.c`.
//!
//! Process liveness, child enumeration, metadata lookup, and recursive
//! tree termination are implemented for Windows and Linux. Other Unix
//! targets retain the original process-group tree-kill behavior but do
//! not yet have platform-specific child/info enumeration.

/// Get immediate child process IDs (`os_proc_children`).
///
/// Returns `(status, children)`: status `0` is success, `1` means the
/// process was not found, and `2` means another error.
#[must_use]
pub fn os_proc_children(ppid: i32) -> (i32, Vec<i32>) {
    if ppid < 0 {
        return (2, Vec::new());
    }
    #[cfg(target_os = "linux")]
    {
        let path = format!("/proc/{ppid}/task/{ppid}/children");
        let Ok(contents) = std::fs::read_to_string(path) else {
            return (2, Vec::new());
        };
        let children = contents
            .split_ascii_whitespace()
            .filter_map(|value| value.parse::<i32>().ok())
            .collect();
        (0, children)
    }
    #[cfg(windows)]
    {
        os_proc_children_windows(ppid)
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        (2, Vec::new())
    }
}

/// Process metadata returned by [`os_proc_info`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcInfo {
    pub name: Vec<u8>,
    pub pid: i32,
    pub ppid: i32,
}

/// Get process name, PID, and parent PID (`os_proc_info` plus the
/// non-Windows `vim._os_proc_info` fallback).
#[must_use]
pub fn os_proc_info(pid: i32) -> Option<ProcInfo> {
    if pid <= 0 {
        return None;
    }

    #[cfg(target_os = "linux")]
    {
        let name = std::fs::read(format!("/proc/{pid}/comm")).ok()?;
        let name = name.strip_suffix(b"\n").unwrap_or(name.as_slice()).to_vec();
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        let after_name = stat.rsplit_once(") ")?.1;
        let mut fields = after_name.split_ascii_whitespace();
        let _state = fields.next()?;
        let ppid = fields.next()?.parse::<i32>().ok()?;
        Some(ProcInfo { name, pid, ppid })
    }
    #[cfg(windows)]
    {
        os_proc_info_windows(pid)
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        None
    }
}

/// Kill a process tree (`os_proc_tree_kill`).
///
/// Unix sends the signal to the process group led by `pid`; Windows
/// recursively terminates descendants before the root.
///
/// # Safety
/// This performs the requested destructive OS process operation.
pub unsafe fn os_proc_tree_kill(pid: i32, signal: i32) -> bool {
    debug_assert!(signal == 15 || signal == 9);
    if pid == 0 {
        return false;
    }
    #[cfg(unix)]
    {
        unsafe { libc::kill(-pid, signal) == 0 }
    }
    #[cfg(windows)]
    {
        os_proc_tree_kill_rec(pid, signal)
    }
    #[cfg(not(any(unix, windows)))]
    {
        false
    }
}

#[cfg(windows)]
fn os_proc_tree_kill_rec(pid: i32, signal: i32) -> bool {
    const PROCESS_ALL_ACCESS: u32 = 0x001f_0fff;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, process_id: u32) -> ProcessHandle;
        fn TerminateProcess(process: ProcessHandle, exit_code: u32) -> i32;
    }
    if pid <= 0 {
        return false;
    }
    let process = unsafe { OpenProcess(PROCESS_ALL_ACCESS, 0, pid as u32) };
    if process.is_null() {
        return false;
    }
    let (_, children) = os_proc_children_windows(pid);
    for child in children {
        let _ = os_proc_tree_kill_rec(child, signal);
    }
    let terminated = unsafe { TerminateProcess(process, signal as u32) } != 0;
    unsafe { CloseHandle(process) };
    terminated
}

#[cfg(windows)]
type ProcessHandle = *mut std::ffi::c_void;
#[cfg(windows)]
const TH32CS_SNAPPROCESS: u32 = 0x0000_0002;
#[cfg(windows)]
const MAX_PATH: usize = 260;
#[cfg(windows)]
const INVALID_HANDLE_VALUE: ProcessHandle = -1isize as ProcessHandle;
#[cfg(windows)]
#[repr(C)]
struct ProcessEntry32 {
    dw_size: u32,
    cnt_usage: u32,
    th32_process_id: u32,
    th32_default_heap_id: usize,
    th32_module_id: u32,
    cnt_threads: u32,
    th32_parent_process_id: u32,
    pc_pri_class_base: i32,
    dw_flags: u32,
    sz_exe_file: [u16; MAX_PATH],
}
#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateToolhelp32Snapshot(flags: u32, process_id: u32) -> ProcessHandle;
    fn Process32FirstW(snapshot: ProcessHandle, entry: *mut ProcessEntry32) -> i32;
    fn Process32NextW(snapshot: ProcessHandle, entry: *mut ProcessEntry32) -> i32;
    fn CloseHandle(handle: ProcessHandle) -> i32;
}

#[cfg(windows)]
fn os_proc_info_windows(pid: i32) -> Option<ProcInfo> {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return None;
    }
    let mut entry = ProcessEntry32 {
        dw_size: std::mem::size_of::<ProcessEntry32>() as u32,
        cnt_usage: 0,
        th32_process_id: 0,
        th32_default_heap_id: 0,
        th32_module_id: 0,
        cnt_threads: 0,
        th32_parent_process_id: 0,
        pc_pri_class_base: 0,
        dw_flags: 0,
        sz_exe_file: [0; MAX_PATH],
    };
    if unsafe { Process32FirstW(snapshot, &mut entry) } == 0 {
        unsafe { CloseHandle(snapshot) };
        return None;
    }
    loop {
        if entry.th32_process_id == pid as u32 {
            let len = entry.sz_exe_file.iter().position(|value| *value == 0).unwrap_or(MAX_PATH);
            let name = String::from_utf16_lossy(&entry.sz_exe_file[..len]).into_bytes();
            let info = ProcInfo { name, pid, ppid: entry.th32_parent_process_id as i32 };
            unsafe { CloseHandle(snapshot) };
            return Some(info);
        }
        if unsafe { Process32NextW(snapshot, &mut entry) } == 0 {
            break;
        }
    }
    unsafe { CloseHandle(snapshot) };
    None
}

#[cfg(windows)]
fn os_proc_children_windows(ppid: i32) -> (i32, Vec<i32>) {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return (2, Vec::new());
    }
    let mut entry = ProcessEntry32 {
        dw_size: std::mem::size_of::<ProcessEntry32>() as u32,
        cnt_usage: 0,
        th32_process_id: 0,
        th32_default_heap_id: 0,
        th32_module_id: 0,
        cnt_threads: 0,
        th32_parent_process_id: 0,
        pc_pri_class_base: 0,
        dw_flags: 0,
        sz_exe_file: [0; MAX_PATH],
    };
    if unsafe { Process32FirstW(snapshot, &mut entry) } == 0 {
        unsafe { CloseHandle(snapshot) };
        return (2, Vec::new());
    }
    let mut children = Vec::new();
    loop {
        if entry.th32_parent_process_id == ppid as u32 {
            children.push(entry.th32_process_id as i32);
        }
        if unsafe { Process32NextW(snapshot, &mut entry) } == 0 {
            break;
        }
    }
    unsafe { CloseHandle(snapshot) };
    (0, children)
}

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
    unsafe extern "system" {
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
    fn os_proc_children_rejects_negative_pid() {
        assert_eq!(os_proc_children(-1), (2, Vec::new()));
    }

    #[test]
    fn os_proc_children_accepts_the_current_process() {
        let (status, _children) = os_proc_children(std::process::id() as i32);
        assert_eq!(status, 0);
    }

    #[test]
    fn os_proc_info_describes_the_current_process() {
        let pid = std::process::id() as i32;
        let info = os_proc_info(pid).expect("current process");
        assert_eq!(info.pid, pid);
        assert!(info.ppid >= 0);
        assert!(!info.name.is_empty());
        assert!(os_proc_info(-1).is_none());
    }

    #[test]
    fn os_proc_tree_kill_never_kills_pid_zero() {
        assert!(!unsafe { os_proc_tree_kill(0, 15) });
        assert!(!unsafe { os_proc_tree_kill(0, 9) });
    }

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
