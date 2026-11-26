//! Windows process queries and owned termination handles.

use super::os_win_console::{OpenProcess, TerminateProcess};
use super::proc_tree::{
    KillOutcome, ProcessAccess, ProcessDriver, ProcessError, ProcessId, ProcessIdentity,
    ProcessRecord, ProcessSignal,
};
use super::proc_windows_data::{decode_process_identity, decode_process_snapshot};
use std::ffi::{c_char, c_void};
use std::io;
use std::os::windows::io::{
    AsHandle, AsRawHandle, BorrowedHandle, FromRawHandle, OwnedHandle, RawHandle,
};
use std::sync::OnceLock;

type NtSystemQuery = unsafe extern "system" fn(u32, *mut c_void, u32, *mut u32) -> i32;
type NtProcessQuery = unsafe extern "system" fn(RawHandle, u32, *mut c_void, u32, *mut u32) -> i32;
type NativeSymbol = unsafe extern "system" fn() -> isize;

unsafe extern "system" {
    fn GetModuleHandleW(name: *const u16) -> RawHandle;
    fn GetProcAddress(module: RawHandle, name: *const c_char) -> Option<NativeSymbol>;
    fn GetExitCodeProcess(handle: RawHandle, status: &mut u32) -> i32;
    fn WaitForSingleObject(handle: RawHandle, millis: u32) -> u32;
}

#[derive(Clone, Copy)]
struct NativeQueries {
    system: NtSystemQuery,
    process: NtProcessQuery,
}

fn native_queries() -> Result<NativeQueries, ProcessError> {
    static QUERIES: OnceLock<Option<NativeQueries>> = OnceLock::new();
    QUERIES
        .get_or_init(|| {
            let name: Vec<u16> = "ntdll.dll\0".encode_utf16().collect();
            // ntdll remains loaded for the process lifetime. Only these documented
            // export signatures are used; no reference into opaque storage is made.
            unsafe {
                let module = GetModuleHandleW(name.as_ptr());
                if module.is_null() {
                    return None;
                }
                let system = GetProcAddress(module, c"NtQuerySystemInformation".as_ptr())?;
                let process = GetProcAddress(module, c"NtQueryInformationProcess".as_ptr())?;
                Some(NativeQueries {
                    system: std::mem::transmute::<NativeSymbol, NtSystemQuery>(system),
                    process: std::mem::transmute::<NativeSymbol, NtProcessQuery>(process),
                })
            }
        })
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::Unsupported,
                "Windows process identity queries are unavailable",
            )
        })
}

enum NativeQuery<'a> {
    Snapshot,
    Identity(BorrowedHandle<'a>),
}

struct NativeBuffer {
    storage: Vec<u64>,
    length: usize,
}

impl NativeBuffer {
    fn as_bytes(&self) -> &[u8] {
        // Every word is initialized, and a successful query bounds length by the
        // supplied byte capacity. Borrowing cannot outlive or resize the storage.
        unsafe { std::slice::from_raw_parts(self.storage.as_ptr().cast::<u8>(), self.length) }
    }
}

fn query_buffer(query: NativeQuery<'_>) -> Result<NativeBuffer, ProcessError> {
    let queries = native_queries()?;
    let mut capacity = 16_384u32;
    for _ in 0..4 {
        let words = (capacity as usize).div_ceil(size_of::<u64>());
        let mut storage = Vec::new();
        storage
            .try_reserve_exact(words)
            .map_err(|_| io::Error::from(io::ErrorKind::OutOfMemory))?;
        storage.resize(words, 0u64);
        let mut written = 0u32;
        // Both calls are synchronous. The aligned, private allocation remains
        // stationary until the call returns; neither API retains its pointers.
        let status = unsafe {
            match query {
                NativeQuery::Snapshot => {
                    (queries.system)(5, storage.as_mut_ptr().cast(), capacity, &mut written)
                }
                NativeQuery::Identity(handle) => (queries.process)(
                    handle.as_raw_handle(),
                    64,
                    storage.as_mut_ptr().cast(),
                    capacity,
                    &mut written,
                ),
            }
        };
        if status >= 0 {
            if written > capacity {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Windows process query exceeded its buffer",
                ));
            }
            return Ok(NativeBuffer {
                storage,
                length: written as usize,
            });
        }
        match status as u32 {
            0x8000_0005 | 0xc000_0004 | 0xc000_0023 => {
                let next = written.max(capacity.saturating_mul(2));
                if next <= capacity {
                    break;
                }
                capacity = next;
            }
            0xc000_0002 | 0xc000_0003 => {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "Windows process identity information is unsupported",
                ));
            }
            _ => {
                return Err(io::Error::other(format!(
                    "Windows process query failed with NTSTATUS {:#010x}",
                    status as u32
                )));
            }
        }
    }
    Err(io::Error::new(
        io::ErrorKind::WouldBlock,
        "Windows process information kept changing during the query",
    ))
}

fn query_process_identity(handle: BorrowedHandle<'_>) -> Result<ProcessIdentity, ProcessError> {
    let buffer = query_buffer(NativeQuery::Identity(handle))?;
    decode_process_identity(buffer.as_bytes())
}

fn query_process_snapshot() -> Result<Vec<ProcessRecord>, ProcessError> {
    let buffer = query_buffer(NativeQuery::Snapshot)?;
    let snapshot = decode_process_snapshot(buffer.as_bytes())?;
    let pid = ProcessId::try_from(i64::from(std::process::id()))
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let current = NativeProcesses
        .open(pid, ProcessAccess::Inspect)?
        .ok_or_else(|| io::Error::other("current process identity is unavailable"))?;
    // The extension is an internal Windows layout. Cross-check it against the
    // separately documented handle query instead of accepting an unknown layout.
    if !snapshot
        .iter()
        .any(|record| record.identity == current.identity)
    {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Windows process snapshot identity is inconsistent",
        ));
    }
    Ok(snapshot)
}

pub(super) struct NativeProcess {
    handle: OwnedHandle,
    identity: ProcessIdentity,
}

pub(super) struct NativeProcesses;

impl ProcessDriver for NativeProcesses {
    type Handle = NativeProcess;

    fn snapshot(&self) -> Result<Vec<ProcessRecord>, ProcessError> {
        query_process_snapshot()
    }

    fn open(
        &self,
        pid: ProcessId,
        access: ProcessAccess,
    ) -> Result<Option<Self::Handle>, ProcessError> {
        const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
        const PROCESS_TERMINATE: u32 = 1;
        const SYNCHRONIZE: u32 = 0x100000;
        let rights = PROCESS_QUERY_LIMITED_INFORMATION
            | match access {
                ProcessAccess::Inspect => 0,
                ProcessAccess::Terminate => PROCESS_TERMINATE | SYNCHRONIZE,
            };
        let raw = unsafe { OpenProcess(rights, 0, pid.get()) };
        if raw.is_null() {
            let error = io::Error::last_os_error();
            return if error.raw_os_error() == Some(87) {
                Ok(None)
            } else {
                Err(error)
            };
        }
        // OpenProcess returned a new, non-inheritable handle. This owner is the
        // sole closer, including every failed metadata query below.
        let handle = unsafe { OwnedHandle::from_raw_handle(raw) };
        let identity = query_process_identity(handle.as_handle())?;
        if identity.pid != pid {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Windows process handle identity is inconsistent",
            ));
        }
        Ok(Some(NativeProcess { handle, identity }))
    }

    fn identity(&self, handle: &Self::Handle) -> ProcessIdentity {
        handle.identity
    }

    fn terminate(
        &self,
        handle: &Self::Handle,
        signal: ProcessSignal,
    ) -> Result<KillOutcome, ProcessError> {
        if unsafe { TerminateProcess(handle.handle.as_raw_handle(), signal.as_raw() as u32) } != 0 {
            return Ok(KillOutcome::Requested);
        }
        let error = io::Error::last_os_error();
        let mut status = 0;
        // As in libuv, check both observations: Windows can publish the exit
        // code before signaling the handle, and an exit code of 259 is valid.
        if (unsafe { GetExitCodeProcess(handle.handle.as_raw_handle(), &mut status) } != 0
            && status != 259)
            || unsafe { WaitForSingleObject(handle.handle.as_raw_handle(), 0) } == 0
        {
            Ok(KillOutcome::AlreadyExited)
        } else {
            Err(error)
        }
    }
}
