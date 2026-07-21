//! Translated from `src/nvim/os/os.h` (partial - just the umbrella
//! header's own small set of declarations, not the `os/*.c` files it
//! pulls in via its "IWYU pragma: begin_exports" generated-header
//! includes, each of which belongs in its own matching Rust module).
//!
//! Translated: `nvim_testing` (true when running under `$NVIM_TEST`)
//! and the `ENV_*` string constants.
//!
//! Deferred: `default_vim_dir`/`default_vimruntime_dir`/
//! `default_lib_dir` - these are plain (non-`EXTERN`) `extern char *`
//! declarations whose real storage and values come from
//! `auto/pathdef.h`, a build-time-generated header containing this
//! checkout's actual install-path strings; translating them faithfully
//! needs that codegen step, not just this header.

use crate::globals::GlobalCell;

/// True if running in a test environment (`$NVIM_TEST`) (`nvim_testing`).
///
/// # TODO (matches the original's own TODO)
/// Can we use `v:testing` instead?
pub static NVIM_TESTING: GlobalCell<bool> = GlobalCell::new(false);

/// `ENV_LOGFILE`
pub const ENV_LOGFILE: &str = "NVIM_LOG_FILE";
/// `ENV_LOGFILE_WANT`
pub const ENV_LOGFILE_WANT: &str = "__NVIM_LOG_FILE_WANT";
/// `ENV_NVIM`
pub const ENV_NVIM: &str = "NVIM";
/// `ENV_RESTART_ALLOC_CONSOLE`
pub const ENV_RESTART_ALLOC_CONSOLE: &str = "__NVIM_RESTART_ALLOC_CONSOLE";
/// `ENV_STARTREASON`
pub const ENV_STARTREASON: &str = "__NVIM_STARTREASON";
/// `ENV_TEST_LOG`
pub const ENV_TEST_LOG: &str = "__NVIM_TEST_LOG";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nvim_testing_defaults_to_false() {
        // Independent of test execution order: only assert the type
        // and that reading it doesn't panic. (Other tests/functions may
        // legitimately flip this at runtime, e.g. `env_init`.)
        let _ = unsafe { *NVIM_TESTING.get_mut() };
    }

    #[test]
    fn env_constants_match_c_macros() {
        assert_eq!(ENV_LOGFILE, "NVIM_LOG_FILE");
        assert_eq!(ENV_NVIM, "NVIM");
    }
}
