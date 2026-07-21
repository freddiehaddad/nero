//! Translated from `src/nvim/os/win_defs.h`.
//! Only compiled on Windows (`IWYU pragma: private, include "nvim/os/os_defs.h"`
//! - the original header itself hard-errors if `MSWIN` isn't defined).

/// `_MAX_PATH` (MSVC CRT), used as the original's `NAME_MAX`.
pub const NAME_MAX: i32 = 260;
/// `_MAX_PATH`, used as the original's `TEMP_FILE_PATH_MAXLEN`.
pub const TEMP_FILE_PATH_MAXLEN: i32 = 260;

/// `TEMP_DIR_NAMES`
pub const TEMP_DIR_NAMES: &[&str] = &["$TMPDIR", "$TMP", "$TEMP", "$USERPROFILE", ""];

pub const FNAME_ILLEGAL: &str = "\"*?><|";

/// Character that separates entries in `$PATH` (`ENV_SEPCHAR`/`ENV_SEPSTR`).
pub const ENV_SEPCHAR: char = ';';
pub const ENV_SEPSTR: &str = ";";

/// `USE_CRNL` flag (defined => true on Windows).
pub const USE_CRNL: bool = true;

/// `BACKSLASH_IN_FILENAME` flag (defined => true on Windows).
pub const BACKSLASH_IN_FILENAME: bool = true;

/// `S_IXUSR` (`#define S_IXUSR S_IEXEC` for MSVC).
pub const S_IXUSR: i32 = libc::S_IEXEC;

/// `SSIZE_MAX`: `_I64_MAX` on 64-bit Windows, `LONG_MAX` on 32-bit.
#[cfg(target_pointer_width = "64")]
pub const SSIZE_MAX: i64 = i64::MAX;
#[cfg(not(target_pointer_width = "64"))]
pub const SSIZE_MAX: i32 = i32::MAX;

/// `O_NOFOLLOW` (undefined on Windows in the original, falls back to 0 -
/// Windows has no symlink-refusal open flag in the traditional CRT API).
pub const O_NOFOLLOW: i32 = 0;

pub const STDIN_FILENO: i32 = 0;
pub const STDOUT_FILENO: i32 = 1;
pub const STDERR_FILENO: i32 = 2;
