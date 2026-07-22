//! Translated from `src/nvim/os/unix_defs.h`.
//! Only compiled on Unix-like targets.

pub const TEMP_DIR_NAMES: &[&str] = &["$TMPDIR", "/tmp", ".", "~"];
pub const TEMP_FILE_PATH_MAXLEN: i32 = 256;

// `HAVE_ACL (HAVE_POSIX_ACL || HAVE_SOLARIS_ACL)` - deferred: depends on
// `auto/config.h` feature detection (build-system probing), not yet
// translated.
// pub const HAVE_ACL: bool = ...;

/// Special wildcards that need to be handled by the shell.
pub const SPECIAL_WILDCHAR: &str = "`'{";

/// Character that separates entries in `$PATH` (`ENV_SEPCHAR`/`ENV_SEPSTR`).
pub const ENV_SEPCHAR: char = ':';
pub const ENV_SEPSTR: &str = ":";
