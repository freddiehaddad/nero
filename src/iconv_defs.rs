//! Translated from `src/nvim/iconv_defs.h`.
//!
//! The real iconv (`<iconv.h>`) binding itself is a vendored third-party
//! dependency (see `cmake.deps/cmake/BuildLibiconv.cmake`), not neovim's own
//! source, so it is not transpiled - it will be reached via FFI/a Rust crate
//! when `mbyte.c` (encoding conversion) is translated. This file only
//! translates the small error-code aliasing the original header adds on top
//! of `<errno.h>`/`<iconv.h>`.

/// `ICONV_ERRNO`: on this target, `errno` is exposed via `libc::errno_location`
/// or platform-specific accessors rather than a plain `errno` global. The
/// original macro is only ever used as `ICONV_ERRNO`, so a function taking
/// its place is used here instead of a global.
#[inline]
pub fn iconv_errno() -> i32 {
    std::io::Error::last_os_error()
        .raw_os_error()
        .unwrap_or(0)
}

pub const ICONV_E2BIG: i32 = libc::E2BIG;
pub const ICONV_EINVAL: i32 = libc::EINVAL;

// `#ifndef EILSEQ` / `#define EILSEQ 123` fallback from the original: some
// platforms' libc lack EILSEQ. The `libc` crate does define `EILSEQ` for the
// Windows/MSVC target this was translated against, so that value is used
// directly (mirroring the "defined" branch, not the "undefined" 123
// fallback, since it genuinely is defined here).
pub const ICONV_EILSEQ: i32 = libc::EILSEQ;
