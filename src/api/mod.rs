//! `src/nvim/api/mod.rs` has no direct C counterpart: it only wires up the
//! `api/` submodule tree in Rust.

pub mod buffer;
pub mod extmark;
pub mod private;
pub mod tabpage;
pub mod vim;
pub mod win_config;
pub mod window;
