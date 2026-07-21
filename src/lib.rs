//! Nero: a literal, file-by-file Rust translation of Neovim's C source
//! (`src/nvim/**/*.c,*.h` in the upstream `neovim/neovim` repository).
//!
//! Each module here corresponds to exactly one C source file. See
//! `S:\projects\git\neovim\src\nvim\<name>.c`/`.h` for the original.
//!
//! `src/nvim/func_attr.h` has no corresponding module: it only defines C
//! function-attribute macros (`FUNC_ATTR_*`) for a preprocessor/codegen
//! mechanism that Rust has no equivalent of; the attributes it controls
//! (malloc, pure, const, nonnull, noreturn, printf-format, ...) are applied
//! directly via native Rust attributes/types at each translated function's
//! definition site instead (e.g. `#[must_use]`, `unsafe fn`, non-null
//! references instead of nullable pointers, `-> !` for noreturn).

pub mod assert_defs;
pub mod gettext_defs;
pub mod macros_defs;
pub mod pos_defs;
pub mod types_defs;
