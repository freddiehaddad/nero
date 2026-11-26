//! Exercise the production process policy without querying or terminating host processes.
#![allow(dead_code)]

#[path = "../../../src/nvim/os/proc_tree.rs"]
mod proc_tree;
#[path = "../../../src/nvim/os/proc_windows_data.rs"]
mod proc_windows_data;

use proc_tree::{
    KillOutcome, ProcessAccess, ProcessDriver, ProcessError, ProcessId, ProcessIdentity,
    ProcessRecord, ProcessSignal, child_identities, os_proc_tree_kill_rec,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::io;
use std::rc::Rc;

fn identity(pid: i64, sequence: u64) -> ProcessIdentity {
    ProcessIdentity {
        pid: ProcessId::try_from(pid).unwrap(),
        sequence,
    }
}

fn record(pid: i64, sequence: u64, parent: Option<i64>) -> ProcessRecord {
    ProcessRecord {
        identity: identity(pid, sequence),
        parent: parent.map(|pid| ProcessId::try_from(pid).unwrap()),
    }
}

#[derive(Default)]
struct State {
    live: HashMap<ProcessId, ProcessIdentity>,
    held: HashMap<ProcessIdentity, usize>,
    opened: usize,
    closed: usize,
    snapshots: usize,
    terminated: Vec<ProcessIdentity>,
}

struct Handle {
    identity: ProcessIdentity,
    state: Rc<RefCell<State>>,
}

impl Drop for Handle {
    fn drop(&mut self) {
        let mut state = self.state.borrow_mut();
        *state.held.get_mut(&self.identity).unwrap() -= 1;
        state.closed += 1;
    }
}

struct Driver {
    rows: Vec<ProcessRecord>,
    state: Rc<RefCell<State>>,
    deny: Option<ProcessId>,
    fail_snapshot: Option<usize>,
}

impl Driver {
    fn new(rows: Vec<ProcessRecord>) -> Self {
        let live = rows
            .iter()
            .map(|row| (row.identity.pid, row.identity))
            .collect();
        Self {
            rows,
            state: Rc::new(RefCell::new(State {
                live,
                ..State::default()
            })),
            deny: None,
            fail_snapshot: None,
        }
    }

    fn root(&self) -> Handle {
        self.open(self.rows[0].identity.pid, ProcessAccess::Terminate)
            .unwrap()
            .unwrap()
    }
}

impl ProcessDriver for Driver {
    type Handle = Handle;

    fn snapshot(&self) -> Result<Vec<ProcessRecord>, ProcessError> {
        let mut state = self.state.borrow_mut();
        state.snapshots += 1;
        assert!(
            state.snapshots <= self.rows.len(),
            "process tree contains a cycle"
        );
        if self.fail_snapshot == Some(state.snapshots) {
            return Err(io::Error::other("snapshot failed"));
        }
        Ok(self.rows.clone())
    }

    fn open(&self, pid: ProcessId, _: ProcessAccess) -> Result<Option<Self::Handle>, ProcessError> {
        if self.deny == Some(pid) {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        let mut state = self.state.borrow_mut();
        let Some(identity) = state.live.get(&pid).copied() else {
            return Ok(None);
        };
        state.opened += 1;
        *state.held.entry(identity).or_default() += 1;
        Ok(Some(Handle {
            identity,
            state: self.state.clone(),
        }))
    }

    fn identity(&self, handle: &Self::Handle) -> ProcessIdentity {
        handle.identity
    }

    fn terminate(
        &self,
        handle: &Self::Handle,
        _: ProcessSignal,
    ) -> Result<KillOutcome, ProcessError> {
        let mut state = self.state.borrow_mut();
        assert_eq!(state.held[&handle.identity], 1);
        if let Some(row) = self.rows.iter().find(|row| row.identity == handle.identity)
            && let Some(parent) = row.parent
        {
            let parent = self
                .rows
                .iter()
                .find(|row| row.identity.pid == parent)
                .unwrap();
            assert_eq!(
                state.held[&parent.identity], 1,
                "parent handle dropped early"
            );
        }
        state.terminated.push(handle.identity);
        Ok(KillOutcome::Requested)
    }
}

#[test]
fn reused_parent_does_not_adopt_older_children() {
    let rows = [
        record(10, 100, None),
        record(20, 10, Some(10)),
        record(30, 101, Some(10)),
    ];
    assert_eq!(
        child_identities(&rows, rows[0].identity).collect::<Vec<_>>(),
        [rows[2].identity]
    );
}

#[test]
fn recycled_candidate_is_not_terminated() {
    let driver = Driver::new(vec![record(10, 100, None), record(20, 101, Some(10))]);
    let root = driver.root();
    let replacement = identity(20, 200);
    driver
        .state
        .borrow_mut()
        .live
        .insert(replacement.pid, replacement);
    os_proc_tree_kill_rec(&driver, root, ProcessSignal::Terminate).unwrap();
    let state = driver.state.borrow();
    assert_eq!(state.terminated, [identity(10, 100)]);
    assert_eq!(state.opened, state.closed);
}

#[test]
fn parents_are_retained_and_terminated_after_descendants() {
    let driver = Driver::new(vec![
        record(10, 100, None),
        record(20, 101, Some(10)),
        record(30, 102, Some(20)),
        record(40, 103, Some(10)),
    ]);
    os_proc_tree_kill_rec(&driver, driver.root(), ProcessSignal::Kill).unwrap();
    let state = driver.state.borrow();
    assert_eq!(
        state.terminated,
        [
            identity(30, 102),
            identity(20, 101),
            identity(40, 103),
            identity(10, 100),
        ]
    );
    assert_eq!((state.opened, state.closed), (4, 4));
}

#[test]
fn failures_do_not_prevent_other_verified_targets_or_leak_handles() {
    let mut driver = Driver::new(vec![
        record(10, 100, None),
        record(20, 101, Some(10)),
        record(30, 102, Some(10)),
    ]);
    driver.deny = Some(identity(20, 101).pid);
    driver.fail_snapshot = Some(2);
    let error = os_proc_tree_kill_rec(&driver, driver.root(), ProcessSignal::Kill).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    let state = driver.state.borrow();
    assert_eq!(state.terminated, [identity(30, 102), identity(10, 100)]);
    assert_eq!((state.opened, state.closed), (2, 2));
}

#[test]
fn protected_stale_children_do_not_require_opening() {
    let mut driver = Driver::new(vec![record(10, 100, None), record(20, 10, Some(10))]);
    driver.deny = Some(identity(20, 10).pid);
    os_proc_tree_kill_rec(&driver, driver.root(), ProcessSignal::Terminate).unwrap();
    let state = driver.state.borrow();
    assert_eq!(state.terminated, [identity(10, 100)]);
    assert_eq!((state.opened, state.closed), (1, 1));
}

fn native_record(pid: u64, parent: u64, sequence: u64) -> Vec<u8> {
    let mut bytes = vec![0; 624];
    bytes[80..88].copy_from_slice(&pid.to_le_bytes());
    bytes[88..96].copy_from_slice(&parent.to_le_bytes());
    bytes[616..624].copy_from_slice(&sequence.to_le_bytes());
    bytes
}

#[test]
fn snapshot_decoder_checks_record_boundaries_and_generations() {
    use proc_windows_data::decode_process_snapshot;

    let mut bytes = native_record(10, 0, u64::MAX - 1);
    let size = bytes.len() as u32;
    bytes[..4].copy_from_slice(&size.to_le_bytes());
    bytes.extend(native_record(20, 10, u64::MAX));
    let rows = decode_process_snapshot(&bytes).unwrap();
    assert_eq!(rows[0].identity, identity(10, u64::MAX - 1));
    assert_eq!(rows[1].parent, Some(identity(10, 0).pid));
    assert_eq!(
        child_identities(&rows, rows[0].identity).collect::<Vec<_>>(),
        [identity(20, u64::MAX)]
    );

    bytes[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(decode_process_snapshot(&bytes).is_err());
    bytes[4..8].copy_from_slice(&0u32.to_le_bytes());
    bytes[..4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(decode_process_snapshot(&bytes).is_err());
    bytes[..4].copy_from_slice(&1u32.to_le_bytes());
    assert!(decode_process_snapshot(&bytes).is_err());
    assert!(decode_process_snapshot(&[]).is_err());
}

#[test]
fn snapshot_decoder_rejects_missing_sequences_and_recycled_records() {
    use proc_windows_data::decode_process_snapshot;

    let mut bytes = native_record(10, 0, 100);
    assert_eq!(
        decode_process_snapshot(&bytes[..616]).unwrap_err().kind(),
        io::ErrorKind::Unsupported
    );
    let size = bytes.len() as u32;
    bytes[..4].copy_from_slice(&size.to_le_bytes());
    bytes.extend(native_record(10, 0, 200));
    assert_eq!(
        decode_process_snapshot(&bytes).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
}

#[test]
fn handle_identity_requires_a_complete_supported_header() {
    use proc_windows_data::decode_process_identity;

    let mut bytes = vec![0; 96];
    bytes[..4].copy_from_slice(&96u32.to_le_bytes());
    bytes[4..8].copy_from_slice(&10u32.to_le_bytes());
    bytes[40..48].copy_from_slice(&u64::MAX.to_le_bytes());
    assert_eq!(
        decode_process_identity(&bytes).unwrap(),
        identity(10, u64::MAX)
    );
    assert!(decode_process_identity(&bytes[..47]).is_err());
    bytes[..4].copy_from_slice(&40u32.to_le_bytes());
    assert!(decode_process_identity(&bytes).is_err());
    bytes[..4].copy_from_slice(&96u32.to_le_bytes());
    bytes[4..8].fill(0);
    assert!(decode_process_identity(&bytes).is_err());
}
