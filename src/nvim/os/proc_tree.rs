//! Process identities and owned process-tree traversal.
#![forbid(unsafe_code)]

use std::fmt;
use std::io;
use std::num::NonZeroU32;

pub type ProcessError = io::Error;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProcessId(NonZeroU32);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidProcessId;

impl fmt::Display for InvalidProcessId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("process ID must be between 1 and i32::MAX")
    }
}

impl std::error::Error for InvalidProcessId {}

impl TryFrom<i64> for ProcessId {
    type Error = InvalidProcessId;

    fn try_from(value: i64) -> Result<Self, Self::Error> {
        u32::try_from(value)
            .ok()
            .and_then(NonZeroU32::new)
            .filter(|value| value.get() <= i32::MAX as u32)
            .map(Self)
            .ok_or(InvalidProcessId)
    }
}

impl ProcessId {
    pub fn get(self) -> u32 {
        self.0.get()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessSignal {
    Interrupt,
    Terminate,
    Kill,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KillOutcome {
    Requested,
    AlreadyExited,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ProcessIdentity {
    pub pid: ProcessId,
    pub sequence: u64,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ProcessRecord {
    pub identity: ProcessIdentity,
    pub parent: Option<ProcessId>,
}

#[derive(Clone, Copy)]
pub(crate) enum ProcessAccess {
    Inspect,
    Terminate,
}

pub(crate) trait ProcessDriver {
    type Handle;

    fn snapshot(&self) -> Result<Vec<ProcessRecord>, ProcessError>;
    fn open(
        &self,
        pid: ProcessId,
        access: ProcessAccess,
    ) -> Result<Option<Self::Handle>, ProcessError>;
    fn identity(&self, handle: &Self::Handle) -> ProcessIdentity;
    fn terminate(
        &self,
        handle: &Self::Handle,
        signal: ProcessSignal,
    ) -> Result<KillOutcome, ProcessError>;
}

pub(crate) fn child_identities(
    snapshot: &[ProcessRecord],
    parent: ProcessIdentity,
) -> impl Iterator<Item = ProcessIdentity> + '_ {
    snapshot
        .iter()
        .filter(move |record| {
            record.parent == Some(parent.pid)
                && record.identity.sequence > parent.sequence
                && record.identity.pid != parent.pid
        })
        .map(|record| record.identity)
}

pub(crate) fn os_proc_tree_kill_rec<D: ProcessDriver>(
    driver: &D,
    root: D::Handle,
    signal: ProcessSignal,
) -> Result<KillOutcome, ProcessError> {
    enum Visit<H> {
        Enter(H),
        Leave(H),
    }

    let mut pending = vec![Visit::Enter(root)];
    let mut failure = None;
    while let Some(visit) = pending.pop() {
        match visit {
            Visit::Enter(handle) => {
                let parent = driver.identity(&handle);
                pending.push(Visit::Leave(handle));
                match driver.snapshot() {
                    Ok(snapshot) => {
                        let mut children = Vec::new();
                        for child in child_identities(&snapshot, parent) {
                            match driver.open(child.pid, ProcessAccess::Terminate) {
                                Ok(Some(handle)) if driver.identity(&handle) == child => {
                                    children.push(handle);
                                }
                                Ok(Some(_)) => {}
                                Ok(None) => {}
                                Err(error) => {
                                    failure.get_or_insert(error);
                                }
                            }
                        }
                        pending.extend(children.into_iter().rev().map(Visit::Enter));
                    }
                    Err(error) => {
                        failure.get_or_insert(error);
                    }
                }
            }
            Visit::Leave(handle) => {
                let result = driver.terminate(&handle, signal);
                if pending.is_empty() {
                    return failure.map_or(result, Err);
                }
                if let Err(error) = result {
                    failure.get_or_insert(error);
                }
            }
        }
    }
    unreachable!("the root is always scheduled for termination")
}
