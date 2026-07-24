//! `src/nvim/eval/mod.rs` has no direct C counterpart: it only wires up
//! the `eval/` submodule tree in Rust.

// `eval::eval` (this crate's own module) matches `src/nvim/eval.c`'s
// exact file stem, per this crate's established "one Rust file mirrors
// one C file's name" convention (see `eval/eval.rs`'s own module doc
// for why it lives here instead of a top-level `src/eval.rs`) - the
// resulting `eval::eval` looks like an accidental self-reference to
// clippy, but is a deliberate, correctly-named exception.
#[allow(clippy::module_inception)]
pub mod eval;
pub mod executor;
pub mod typval;
pub mod typval_defs;
pub mod userfunc;
pub mod vars;
