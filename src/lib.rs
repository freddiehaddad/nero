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

pub mod api;
pub mod arabic;
pub mod arglist_defs;
pub mod ascii_defs;
pub mod assert_defs;
pub mod base64;
pub mod buffer;
pub mod buffer_defs;
pub mod change;
pub mod charset;
pub mod cursor;
pub mod decoration_defs;
pub mod drawscreen;
pub mod errors;
pub mod eval;
pub mod ex_cmds_defs;
pub mod ex_eval_defs;
pub mod extmark_defs;
pub mod fold_defs;
pub mod fuzzy;
pub mod garray;
pub mod garray_defs;
pub mod gettext_defs;
pub mod globals;
pub mod grid_defs;
pub mod hashtab;
pub mod hashtab_defs;
pub mod highlight_defs;
pub mod iconv_defs;
pub mod indent;
pub mod input_defs;
pub mod insert_defs;
pub mod log;
pub mod macros_defs;
pub mod map;
pub mod mark;
pub mod mark_defs;
pub mod marktree;
pub mod marktree_defs;
pub mod math;
pub mod mbyte;
pub mod mbyte_defs;
pub mod memfile;
pub mod memfile_defs;
pub mod memline;
pub mod memline_defs;
pub mod memory;
pub mod memory_defs;
pub mod menu_defs;
#[path = "move.rs"]
pub mod r#move;
pub mod normal_defs;
pub mod option;
pub mod option_defs;
pub mod option_vars;
pub mod os;
pub mod path;
pub mod plines;
pub mod pos_defs;
pub mod profile;
pub mod regexp_defs;
pub mod runtime_defs;
pub mod search_defs;
pub mod sha256;
pub mod sign_defs;
pub mod state;
pub mod state_defs;
pub mod statusline_defs;
pub mod strings;
pub mod types_defs;
pub mod undo;
pub mod undo_defs;
pub mod vim_defs;
pub mod window;
