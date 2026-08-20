//! Rust module wiring for `src/nvim/viml/parser/`.

// The nested name preserves `viml/parser/parser.c`'s real path.
#[allow(clippy::module_inception)]
pub mod parser;
pub mod parser_defs;
