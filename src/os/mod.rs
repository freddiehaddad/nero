//! `src/nvim/os/mod.rs` has no direct C counterpart: it exists only to wire
//! up the `os/` submodule tree in Rust (`src/nvim/os/` has no single
//! umbrella header of its own in the original - each `os/*.c` file includes
//! exactly the `os/*.h` it needs).

pub mod env;
pub mod fs;
pub mod fs_defs;
// The original `src/nvim/os/os.h` really does live inside the `os/`
// directory, so this module naturally mirrors it as `os::os` - kept
// despite the lint since renaming it would break the file-mirroring
// convention this crate otherwise follows everywhere.
#[allow(clippy::module_inception)]
pub mod os;
pub mod os_defs;
pub mod proc;
pub mod stdpaths;
pub mod time;
pub mod time_defs;
#[cfg(unix)]
pub mod unix_defs;
#[cfg(windows)]
pub mod win_defs;
