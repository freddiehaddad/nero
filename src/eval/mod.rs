//! `src/nvim/eval/mod.rs` has no direct C counterpart: it only wires up
//! the `eval/` submodule tree in Rust.

pub mod typval;
pub mod typval_defs;
pub mod userfunc;
