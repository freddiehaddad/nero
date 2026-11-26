//! Checked decoding of native 64-bit Windows process identity records.
#![forbid(unsafe_code)]

use super::proc_tree::{ProcessError, ProcessId, ProcessIdentity, ProcessRecord};
use std::collections::HashSet;
use std::io;

// SYSTEM_PROCESS_INFORMATION, SYSTEM_THREAD_INFORMATION, and the Windows 10
// RS3+ SYSTEM_PROCESS_INFORMATION_EXTENSION. Only IDs and the sequence are read.
const PROCESS_HEADER_SIZE: usize = 256;
const THREAD_INFO_SIZE: usize = 80;
const SEQUENCE_OFFSET: usize = 360;

fn field<const N: usize>(bytes: &[u8], offset: usize) -> Result<[u8; N], ProcessError> {
    offset
        .checked_add(N)
        .and_then(|end| bytes.get(offset..end))
        .and_then(|value| value.try_into().ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "truncated process information"))
}

fn process_id(value: u64) -> Result<Option<ProcessId>, ProcessError> {
    if value == 0 {
        return Ok(None);
    }
    let value = i64::try_from(value)
        .ok()
        .and_then(|value| ProcessId::try_from(value).ok());
    value.map(Some).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "native process ID is out of range",
        )
    })
}

pub(crate) fn decode_process_snapshot(bytes: &[u8]) -> Result<Vec<ProcessRecord>, ProcessError> {
    let mut processes = Vec::new();
    let mut seen = HashSet::new();
    let mut offset = 0usize;
    loop {
        let next = u32::from_le_bytes(field(bytes, offset)?) as usize;
        let end = if next == 0 {
            bytes.len()
        } else {
            offset
                .checked_add(next)
                .filter(|&end| end <= bytes.len())
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "invalid process record offset")
                })?
        };
        let record = bytes
            .get(offset..end)
            .filter(|record| record.len() >= PROCESS_HEADER_SIZE)
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "invalid process record length")
            })?;
        let threads = u32::from_le_bytes(field(record, 4)?) as usize;
        let sequence_offset = threads
            .checked_mul(THREAD_INFO_SIZE)
            .and_then(|size| size.checked_add(PROCESS_HEADER_SIZE + SEQUENCE_OFFSET))
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "invalid process thread count")
            })?;
        let sequence = field(record, sequence_offset).map_err(|_| {
            io::Error::new(
                io::ErrorKind::Unsupported,
                "Windows process sequence information is unavailable",
            )
        })?;
        if let Some(pid) = process_id(u64::from_le_bytes(field(record, 80)?))? {
            if !seen.insert(pid) {
                return Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "process IDs changed during enumeration",
                ));
            }
            processes.push(ProcessRecord {
                identity: ProcessIdentity {
                    pid,
                    sequence: u64::from_le_bytes(sequence),
                },
                parent: process_id(u64::from_le_bytes(field(record, 88)?))?,
            });
        }
        if next == 0 {
            return Ok(processes);
        }
        offset = end;
    }
}

pub(crate) fn decode_process_identity(bytes: &[u8]) -> Result<ProcessIdentity, ProcessError> {
    // PROCESS_TELEMETRY_ID_INFORMATION has a versioned header; the sequence ends
    // at byte 48. Later header members and variable-length strings are not used.
    let header_size = u32::from_le_bytes(field(bytes, 0)?) as usize;
    if header_size < 48 || header_size > bytes.len() {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Windows process identity header is unavailable",
        ));
    }
    let pid = process_id(u64::from(u32::from_le_bytes(field(bytes, 4)?)))?
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing process identity"))?;
    Ok(ProcessIdentity {
        pid,
        sequence: u64::from_le_bytes(field(bytes, 40)?),
    })
}
