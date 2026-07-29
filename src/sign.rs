//! Translated from `src/nvim/sign.c` (tractable core only).
//!
//! `sign.c` (~1650 lines) implements the `:sign` command family
//! (`:sign define`/`place`/`unplace`/`list`/`jump`) and its Vimscript
//! `sign_*()` builtin-function counterparts - almost every function
//! needs the real sign-registry/placed-sign-list machinery (`first_sign`,
//! `buf->b_signlist`) and/or the Ex-command execution engine, neither
//! translated.
//!
//! Translated: `sign_cmd_idx` (find a `":sign"` subcommand's index by
//! name, via the small, fixed `cmds` table). No real caller yet
//! (`ex_sign`, its only reader, isn't translated) - translated ahead
//! of it anyway, matching this crate's established "translate a
//! small, simple, mechanically-correct piece ahead of the surrounding
//! engine" precedent.
//!
//! Deferred: everything else in the file.

/// `":sign"` subcommand names, in `SIGNCMD_*` order (`cmds`).
const CMDS: [&str; 6] = ["define", "undefine", "list", "place", "unplace", "jump"];

/// `":sign define"` (`SIGNCMD_DEFINE`).
pub const SIGNCMD_DEFINE: i32 = 0;
/// `":sign undefine"` (`SIGNCMD_UNDEFINE`).
pub const SIGNCMD_UNDEFINE: i32 = 1;
/// `":sign list"` (`SIGNCMD_LIST`).
pub const SIGNCMD_LIST: i32 = 2;
/// `":sign place"` (`SIGNCMD_PLACE`).
pub const SIGNCMD_PLACE: i32 = 3;
/// `":sign unplace"` (`SIGNCMD_UNPLACE`).
pub const SIGNCMD_UNPLACE: i32 = 4;
/// `":sign jump"` (`SIGNCMD_JUMP`).
pub const SIGNCMD_JUMP: i32 = 5;
/// One past the last real `SIGNCMD_*` value - returned by
/// [`sign_cmd_idx`] when no subcommand name matches (`SIGNCMD_LAST`).
pub const SIGNCMD_LAST: i32 = 6;

/// Find the index of a `":sign"` subcommand from its name
/// (`sign_cmd_idx`). Returns [`SIGNCMD_LAST`] if `cmd` doesn't match
/// any known subcommand name.
///
/// The original takes `begin_cmd`/`end_cmd` pointers into a shared
/// buffer, temporarily NUL-terminating at `end_cmd` (restoring the
/// original character afterward) to compare just the subcommand
/// portion - this crate's own byte slice already carries its own
/// bound, so `cmd` is simply the already-isolated subcommand text
/// directly, with no NUL-poking/restoring needed.
#[must_use]
pub fn sign_cmd_idx(cmd: &[u8]) -> i32 {
    for (idx, name) in CMDS.iter().enumerate() {
        if cmd == name.as_bytes() {
            return idx as i32;
        }
    }
    SIGNCMD_LAST
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_every_real_subcommand_name() {
        assert_eq!(sign_cmd_idx(b"define"), SIGNCMD_DEFINE);
        assert_eq!(sign_cmd_idx(b"undefine"), SIGNCMD_UNDEFINE);
        assert_eq!(sign_cmd_idx(b"list"), SIGNCMD_LIST);
        assert_eq!(sign_cmd_idx(b"place"), SIGNCMD_PLACE);
        assert_eq!(sign_cmd_idx(b"unplace"), SIGNCMD_UNPLACE);
        assert_eq!(sign_cmd_idx(b"jump"), SIGNCMD_JUMP);
    }

    #[test]
    fn unrecognized_name_returns_signcmd_last() {
        assert_eq!(sign_cmd_idx(b"bogus"), SIGNCMD_LAST);
        assert_eq!(sign_cmd_idx(b""), SIGNCMD_LAST);
    }

    #[test]
    fn is_case_sensitive_and_requires_an_exact_match() {
        // "Define" (wrong case) and "def" (a mere prefix) both fail -
        // the original's own strcmp is a full, case-sensitive match.
        assert_eq!(sign_cmd_idx(b"Define"), SIGNCMD_LAST);
        assert_eq!(sign_cmd_idx(b"def"), SIGNCMD_LAST);
        assert_eq!(sign_cmd_idx(b"defined"), SIGNCMD_LAST);
    }
}
