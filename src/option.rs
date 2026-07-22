//! Translated from `src/nvim/option.c` (tractable core only).
//!
//! `option.c` is a massive (~6897-line) file implementing the entire
//! `:set`/options-parsing engine, deeply entangled with the eval
//! engine, autocmd triggers, and nearly every other subsystem. Only
//! this one genuinely standalone, pure function is translated so far -
//! harvested because it directly unblocks part of `memline.c`'s
//! `ml_open` (still blocked overall by `mf_open`/`iemsg`, but this
//! removes one of its several real dependencies):
//!
//! Translated: `get_fileformat`.
//!
//! Deferred: everything else, including `get_fileformat_force` (needs
//! `exarg_T`, blocked on the `ex_cmds.lua`-generated `cmdidx_T` - same
//! blocker as `mark.c`'s `ex_*` functions).

use crate::buffer_defs::BufT;
use crate::option_vars::{EOL_DOS, EOL_MAC, EOL_UNIX};

/// Gets the `'fileformat'` of `buf` as an `EOL_*` constant
/// (`get_fileformat`).
#[must_use]
pub fn get_fileformat(buf: &BufT) -> i32 {
    let c = buf
        .b_p_ff
        .as_deref()
        .and_then(|s| s.first())
        .copied()
        .unwrap_or(0);

    if buf.b_p_bin != 0 || c == b'u' {
        return EOL_UNIX;
    }
    if c == b'm' {
        return EOL_MAC;
    }
    EOL_DOS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buf_with_ff(ff: &str, bin: bool) -> BufT {
        BufT {
            b_p_ff: Some(ff.as_bytes().to_vec()),
            b_p_bin: i32::from(bin),
            ..Default::default()
        }
    }

    #[test]
    fn get_fileformat_unix() {
        assert_eq!(get_fileformat(&buf_with_ff("unix", false)), EOL_UNIX);
    }

    #[test]
    fn get_fileformat_mac() {
        assert_eq!(get_fileformat(&buf_with_ff("mac", false)), EOL_MAC);
    }

    #[test]
    fn get_fileformat_dos() {
        assert_eq!(get_fileformat(&buf_with_ff("dos", false)), EOL_DOS);
    }

    #[test]
    fn get_fileformat_binary_forces_unix() {
        assert_eq!(get_fileformat(&buf_with_ff("dos", true)), EOL_UNIX);
    }

    #[test]
    fn get_fileformat_empty_ff_defaults_to_dos() {
        let buf = BufT::default(); // b_p_ff is None
        assert_eq!(get_fileformat(&buf), EOL_DOS);
    }
}
