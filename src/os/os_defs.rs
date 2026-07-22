//! Translated from `src/nvim/os/os_defs.h`.
//!
//! Deferred (need other subsystems not yet translated):
//! - `os_strerror`/`os_translate_sys_error` (aliases for libuv's
//!   `uv_strerror`/`uv_translate_sys_error`): phase 11 (event/, libuv
//!   decision).
//! - `os_strtok` (`strtok_s`/`strtok_r` wrapper): translated at its first
//!   real call site.
//!
//! N/A (superseded natively by Rust's stdlib, not a wrapper we need to
//! write): `off_T`/`vim_lseek`/`vim_fseek`/`vim_ftell` - `std::io::Seek`
//! already uses a 64-bit offset (`i64`) uniformly on every platform Rust
//! supports; C's split between 32-bit and 64-bit file APIs doesn't exist
//! here.
//!
//! This crate's default/CI-verified target is Windows (this dev
//! machine); Unix-only code here is additionally cross-checked via
//! `cargo check --target x86_64-unknown-linux-gnu` (no native Unix
//! machine available). Several `as i32`/`as u32` casts below are
//! genuinely redundant specifically on that Linux target (`clippy`
//! flags them there) because `libc`'s `NAME_MAX`/`PATH_MAX`/`S_IFMT`-
//! family constants already happen to be that exact type on Linux -
//! but their type isn't guaranteed to be identical across every other
//! Unix `libc` supports (macOS, the BSDs), so the explicit casts are
//! kept intentionally for portability beyond the one Unix flavor this
//! crate can actually cross-check, rather than chasing a single
//! target's clippy output.

#[cfg(windows)]
pub use crate::os::win_defs::*;
#[cfg(unix)]
pub use crate::os::unix_defs::*;

/// `BACKSLASH_IN_FILENAME_BOOL`
#[cfg(windows)]
pub const BACKSLASH_IN_FILENAME_BOOL: bool = true;
#[cfg(not(windows))]
pub const BACKSLASH_IN_FILENAME_BOOL: bool = false;

/// `NAME_MAX`. On Unix, the original expects this already defined by
/// the system's own `<limits.h>` (transitively included via
/// `<sys/stat.h>`/`<stdlib.h>`), falling back to `_XOPEN_NAME_MAX`
/// only on the rare system that lacks it - a fallback that virtually
/// never fires on any real glibc/musl/BSD/macOS libc, all of which
/// define `NAME_MAX` directly. `libc::NAME_MAX` already reflects
/// whatever the real system defines, so it's used directly here
/// rather than replicating that rarely-hit fallback. (Windows'
/// `NAME_MAX` is instead its own direct definition, `_MAX_PATH`,
/// already provided by the `win_defs::*` glob import above.)
#[cfg(unix)]
pub const NAME_MAX: i32 = libc::NAME_MAX as i32;

/// `BASENAMELEN` (`NAME_MAX - 5`)
pub const BASENAMELEN: i32 = NAME_MAX - 5;

/// Use the system path length if it makes sense (`MAXPATHL`). On Windows the
/// original's `#if defined(PATH_MAX) && (PATH_MAX > DEFAULT_MAXPATHL)` never
/// fires (MSVC's CRT headers don't define `PATH_MAX`), so it always falls
/// back to `DEFAULT_MAXPATHL` there.
pub const DEFAULT_MAXPATHL: i32 = 4096;
#[cfg(windows)]
pub const MAXPATHL: i32 = DEFAULT_MAXPATHL;
#[cfg(unix)]
pub const MAXPATHL: i32 = if (libc::PATH_MAX as i32) > DEFAULT_MAXPATHL {
    libc::PATH_MAX as i32
} else {
    DEFAULT_MAXPATHL
};

/// Command-processing buffer. Use large buffers for all platforms.
pub const CMDBUFFSIZE: usize = 1024;

pub const ROOT_UID: u32 = 0;

/// `S_ISDIR(m)`, `S_ISREG(m)`, etc: the `libc` crate already exposes the
/// underlying `S_IF*`/`S_IFMT` constants portably, so these are implemented
/// directly rather than left as macros.
#[inline]
pub fn s_isdir(m: u32) -> bool {
    (m & libc::S_IFMT as u32) == libc::S_IFDIR as u32
}
#[inline]
pub fn s_isreg(m: u32) -> bool {
    (m & libc::S_IFMT as u32) == libc::S_IFREG as u32
}
#[inline]
pub fn s_ischr(m: u32) -> bool {
    (m & libc::S_IFMT as u32) == libc::S_IFCHR as u32
}
#[cfg(unix)]
#[inline]
pub fn s_isblk(m: u32) -> bool {
    (m & libc::S_IFMT as u32) == libc::S_IFBLK as u32
}
#[cfg(unix)]
#[inline]
pub fn s_isfifo(m: u32) -> bool {
    (m & libc::S_IFMT as u32) == libc::S_IFIFO as u32
}
#[cfg(unix)]
#[inline]
pub fn s_islnk(m: u32) -> bool {
    (m & libc::S_IFMT as u32) == libc::S_IFLNK as u32
}
#[cfg(unix)]
#[inline]
pub fn s_issock(m: u32) -> bool {
    (m & libc::S_IFMT as u32) == libc::S_IFSOCK as u32
}
// Windows has no S_IFBLK/S_IFIFO/S_IFLNK/S_IFSOCK in the CRT's <sys/stat.h>;
// the original's `#ifndef S_ISBLK ... #define S_ISBLK(m) 0` fallback applies
// there (libuv defines S_IFLNK itself when needed, per a comment in
// ascii_defs.h - handled when that call site is translated).
#[cfg(windows)]
#[inline]
pub fn s_isblk(_m: u32) -> bool {
    false
}
#[cfg(windows)]
#[inline]
pub fn s_isfifo(_m: u32) -> bool {
    false
}
#[cfg(windows)]
#[inline]
pub fn s_islnk(_m: u32) -> bool {
    false
}
#[cfg(windows)]
#[inline]
pub fn s_issock(_m: u32) -> bool {
    false
}
