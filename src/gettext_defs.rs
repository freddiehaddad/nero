//! Translated from `src/nvim/gettext_defs.h`.
//!
//! The original header has two compile-time modes selected by
//! `HAVE_WORKING_LIBINTL`. Rust has no C preprocessor, so the equivalent
//! mechanism is a Cargo feature (`libintl`) selected at compile time -
//! mirroring the `#ifdef`/`#else` exactly rather than inventing a new
//! abstraction.
//!
//! Only the `#else` (no libintl) branch is implemented so far: `_(x) = x`,
//! `N_(x) = x`, `NGETTEXT(x, xs, n) = if n == 1 { x } else { xs }`, and the
//! `bindtextdomain`/`bind_textdomain_codeset`/`textdomain` no-ops. This is
//! not a stand-in fake - it is one of the two real code paths in the
//! original header. The `HAVE_WORKING_LIBINTL` branch (real libintl FFI:
//! `gettext`/`ngettext`/`bindtextdomain`/`bind_textdomain_codeset`/
//! `textdomain`) is deferred - not yet wired to link against libintl (see
//! `cmake.deps/cmake/BuildGettext.cmake`, `ENABLE_LIBINTL` in
//! `src/nvim/CMakeLists.txt`).

#[cfg(not(feature = "libintl"))]
mod imp {
    /// `_(x)` macro: translate a string. No-libintl fallback: identity.
    #[inline]
    pub fn gettext(x: &str) -> &str {
        x
    }

    /// `N_(x)` macro: mark a string as translatable without translating it
    /// now (used for strings translated later, e.g. table initializers).
    /// No-libintl fallback: identity.
    #[inline]
    pub fn gettext_noop(x: &str) -> &str {
        x
    }

    /// `NGETTEXT(x, xs, n)` macro: plural-aware translation. No-libintl
    /// fallback: `n == 1 ? x : xs`.
    #[inline]
    pub fn ngettext<'a>(x: &'a str, xs: &'a str, n: u64) -> &'a str {
        if n == 1 {
            x
        } else {
            xs
        }
    }

    /// `bindtextdomain(x, y)` - no-op without libintl.
    #[inline]
    pub fn bindtextdomain(_domain: &str, _dir: &str) {}

    /// `bind_textdomain_codeset(x, y)` - no-op without libintl.
    #[inline]
    pub fn bind_textdomain_codeset(_domain: &str, _codeset: &str) {}

    /// `textdomain(x)` - no-op without libintl.
    #[inline]
    pub fn textdomain(_domain: &str) {}
}

#[cfg(feature = "libintl")]
mod imp {
    // TODO(nero): real libintl FFI (gettext/ngettext/bindtextdomain/
    // bind_textdomain_codeset/textdomain), matching the
    // `HAVE_WORKING_LIBINTL` branch of `src/nvim/gettext_defs.h`. Deferred
    // until the build is wired to link libintl like
    // `cmake.deps/cmake/BuildGettext.cmake` does.
    compile_error!("the `libintl` feature is not implemented yet");
}

pub use imp::*;
