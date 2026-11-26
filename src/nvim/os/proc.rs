//! OS process inspection and process-tree termination (`src/nvim/os/proc.c`).
pub use super::proc_tree::{KillOutcome, ProcessError, ProcessId, ProcessSignal};
#[cfg(windows)]
use super::proc_tree::{ProcessDriver, child_identities, os_proc_tree_kill_rec};
#[cfg(windows)]
use super::proc_windows::NativeProcesses;
#[cfg(windows)]
use crate::eval::funcs::PROCESS_ALL_ACCESS;
use crate::event::r#loop::uv_kill;
#[cfg(windows)]
use std::ffi::CStr;
#[cfg(target_os = "windows")]
use std::ffi::c_void;

#[cfg(target_os = "windows")]
use crate::api::private::validate::{Dict, NvimString};
#[cfg(windows)]
use crate::api::private::validate::{KeyValuePair, Object};

#[cfg(target_os = "windows")]
const UV_ESRCH: i32 = -4040;
#[cfg(not(target_os = "windows"))]
const UV_ESRCH: i32 = -3;

impl ProcessSignal {
    pub fn as_raw(self) -> i32 {
        match self {
            Self::Interrupt => crate::os::signal::SIGINT,
            Self::Terminate => crate::os::signal::SIGTERM,
            Self::Kill => crate::os::signal::SIGKILL,
        }
    }
}

#[cfg(target_os = "windows")]
const TH32CS_SNAPPROCESS: u32 = 0x0000_0002;
#[cfg(target_os = "windows")]
const INVALID_HANDLE_VALUE: *mut c_void = (-1isize) as *mut c_void;

#[cfg(target_os = "windows")]
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
    sz_exe_file: [u8; 260],
}

#[cfg(target_os = "windows")]
impl ProcessEntry32 {
    const fn new() -> Self {
        Self {
            dw_size: std::mem::size_of::<Self>() as u32,
            cnt_usage: 0,
            th32_process_id: 0,
            th32_default_heap_id: 0,
            th32_module_id: 0,
            cnt_threads: 0,
            th32_parent_process_id: 0,
            pc_pri_class_base: 0,
            dw_flags: 0,
            sz_exe_file: [0; 260],
        }
    }
}

#[cfg(target_os = "windows")]
impl Default for ProcessEntry32 {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_os = "windows")]
use crate::os::os_win_console::{CloseHandle, DebugBreakProcess, OpenProcess};

pub fn os_debugbreak(pid: i32) -> bool {
    #[cfg(windows)]
    {
        let h_process = unsafe { OpenProcess(PROCESS_ALL_ACCESS, 0, pid as u32) };
        if h_process.is_null() {
            return false;
        }
        unsafe { DebugBreakProcess(h_process) };
        unsafe { CloseHandle(h_process) };
        true
    }
    #[cfg(not(windows))]
    {
        unsafe { uv_kill(pid, crate::os::signal::SIGINT) };
        false
    }
}

#[cfg(target_os = "windows")]
unsafe extern "system" {
    fn CreateToolhelp32Snapshot(dw_flags: u32, th32_process_id: u32) -> *mut c_void;
    fn Process32First(h_snapshot: *mut c_void, lppe: *mut ProcessEntry32) -> i32;
    fn Process32Next(h_snapshot: *mut c_void, lppe: *mut ProcessEntry32) -> i32;
}

/// Kill process `pid` and its process group / descendants (`os_proc_tree_kill`).
pub fn os_proc_tree_kill(
    pid: ProcessId,
    signal: ProcessSignal,
) -> Result<KillOutcome, ProcessError> {
    #[cfg(target_os = "windows")]
    {
        let Some(root) = NativeProcesses.open(pid, super::proc_tree::ProcessAccess::Terminate)?
        else {
            return Ok(KillOutcome::AlreadyExited);
        };
        os_proc_tree_kill_rec(&NativeProcesses, root, signal)
    }

    #[cfg(not(target_os = "windows"))]
    {
        match unsafe { uv_kill(-(pid.get() as i32), signal.as_raw()) } {
            0 => Ok(KillOutcome::Requested),
            UV_ESRCH => Ok(KillOutcome::AlreadyExited),
            error => Err(std::io::Error::from_raw_os_error(-error)),
        }
    }
}

/// Get the immediate child PIDs of `ppid` (`os_proc_children`).
pub fn os_proc_children(parent: ProcessId) -> Result<Option<Vec<ProcessId>>, ProcessError> {
    #[cfg(windows)]
    {
        let snapshot = NativeProcesses.snapshot()?;
        let Some(parent) = snapshot.iter().find(|row| row.identity.pid == parent) else {
            return Ok(None);
        };
        Ok(Some(
            child_identities(&snapshot, parent.identity)
                .map(|child| child.pid)
                .collect(),
        ))
    }

    #[cfg(not(windows))]
    {
        let ppid = parent.get() as i32;
        #[allow(unused_mut)]
        let mut temp = Vec::new();

        #[cfg(target_os = "macos")]
        unsafe {
            unsafe extern "C" {
                fn sysctl(
                    name: *mut i32,
                    namelen: u32,
                    oldp: *mut std::ffi::c_void,
                    oldlenp: *mut usize,
                    newp: *mut std::ffi::c_void,
                    newlen: usize,
                ) -> i32;
            }
            const CTL_KERN: i32 = 1;
            const KERN_PROC: i32 = 14;
            const KERN_PROC_ALL: i32 = 0;
            const KINFO_PROC_SIZE: usize = 648;
            const KP_PID_OFFSET: usize = 40;
            const KP_PPID_OFFSET: usize = 560;

            let mut name = [CTL_KERN, KERN_PROC, KERN_PROC_ALL, 0];
            let mut len: usize = 0;
            if sysctl(
                name.as_mut_ptr(),
                3,
                std::ptr::null_mut(),
                &mut len,
                std::ptr::null_mut(),
                0,
            ) != 0
            {
                return Err(std::io::Error::last_os_error());
            }

            let mut buf = vec![0u8; len];
            if sysctl(
                name.as_mut_ptr(),
                3,
                buf.as_mut_ptr().cast(),
                &mut len,
                std::ptr::null_mut(),
                0,
            ) != 0
            {
                return Err(std::io::Error::last_os_error());
            }

            let mut exists = false;
            let p_count = len / KINFO_PROC_SIZE;
            for i in 0..p_count {
                let entry = &buf[i * KINFO_PROC_SIZE..(i + 1) * KINFO_PROC_SIZE];
                let pid =
                    i32::from_ne_bytes(entry[KP_PID_OFFSET..KP_PID_OFFSET + 4].try_into().unwrap());
                let parent_pid = i32::from_ne_bytes(
                    entry[KP_PPID_OFFSET..KP_PPID_OFFSET + 4]
                        .try_into()
                        .unwrap(),
                );
                exists = exists || pid == ppid;
                if parent_pid == ppid
                    && let Ok(pid) = ProcessId::try_from(i64::from(pid))
                {
                    temp.push(pid);
                }
            }
            if !exists {
                return Ok(None);
            }
        }

        #[cfg(target_os = "linux")]
        {
            let path = format!("/proc/{ppid}/task/{ppid}/children");
            let contents = std::fs::read_to_string(&path)?;
            for tok in contents.split_ascii_whitespace() {
                if let Ok(match_pid) = tok.parse::<i64>()
                    && let Ok(match_pid) = ProcessId::try_from(match_pid)
                {
                    temp.push(match_pid);
                }
            }
        }

        Ok(Some(temp))
    }
}

#[cfg(target_os = "windows")]
/// Return a dictionary of process properties (`pid`, `ppid`, `name`) on Windows (`os_proc_info`).
///
/// # Safety
/// `arena` must be a valid pointer to an initialized `Arena`.
pub fn os_proc_info(pid: i32) -> Dict {
    let mut pinfo = Dict::new();

    {
        let h = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if h == INVALID_HANDLE_VALUE {
            return pinfo;
        }
        let mut pe = ProcessEntry32::new();
        if unsafe { Process32First(h, &mut pe) } == 0 {
            unsafe { CloseHandle(h) };
            return pinfo;
        }
        loop {
            if pe.th32_process_id == pid as u32 {
                break;
            }
            if unsafe { Process32Next(h, &mut pe) } == 0 {
                break;
            }
        }
        unsafe { CloseHandle(h) };

        if pe.th32_process_id == pid as u32 {
            pinfo = Dict::with_capacity(3);
            let put = |dict: &mut Dict, key: &CStr, value: Object| {
                let idx = dict.size;
                dict[idx] = KeyValuePair {
                    key: NvimString::from_cstr(key),
                    value,
                };
                dict.size += 1;
            };

            put(&mut pinfo, c"pid", Object::Integer(i64::from(pid)));
            put(
                &mut pinfo,
                c"ppid",
                Object::Integer(i64::from(pe.th32_parent_process_id as i32)),
            );
            let exe_cstr = CStr::from_bytes_until_nul(&pe.sz_exe_file).unwrap_or(c"");
            let name_str = NvimString::from_bytes(exe_cstr.to_bytes());
            put(&mut pinfo, c"name", Object::String(name_str));
        }

        pinfo
    }
}

/// Return true if process `pid` is running (`os_proc_running`).
pub fn os_proc_running(pid: i32) -> bool {
    let err = unsafe { uv_kill(pid, 0) };
    if err == 0 {
        return true;
    }
    if err == UV_ESRCH {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negative_ppid_returns_error_and_running_checks_pid() {
        assert_eq!(
            ProcessId::try_from(-1),
            Err(crate::os::proc_tree::InvalidProcessId)
        );
        assert!(os_proc_running(std::process::id() as i32));
        assert!(!os_proc_running(i32::MAX));
    }
}
