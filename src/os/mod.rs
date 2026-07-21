//! `src/nvim/os/mod.rs` has no direct C counterpart: it exists only to wire
//! up the `os/` submodule tree in Rust (`src/nvim/os/` has no single
//! umbrella header of its own in the original - each `os/*.c` file includes
//! exactly the `os/*.h` it needs).

pub mod os_defs;
pub mod time_defs;
#[cfg(unix)]
pub mod unix_defs;
#[cfg(windows)]
pub mod win_defs;
