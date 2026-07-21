//! Translated from `src/nvim/search_defs.h`.

use crate::pos_defs::{ColnrT, LinenrT};

/// Search/highlight subsystem state (`SearchState`).
#[derive(Debug, Clone, Copy, Default)]
pub struct SearchState {
    /// Highlight the match, starting at cursor pos.
    pub hl_match: bool,
    /// Lines after the match (0 for a match within one line).
    pub match_lines: LinenrT,
    /// Column just after the match in the last line.
    pub match_endcol: ColnrT,
    /// For `:{FIRST},{last}s/pat`.
    pub first_line: LinenrT,
    /// For `:{first},{LAST}s/pat`.
    pub last_line: LinenrT,
    /// Don't use `'smartcase'` once.
    pub no_smartcase: bool,
    /// Length of previous search cmd.
    pub cmdlen: i32,
    /// Don't use `'hlsearch'` temporarily.
    pub no_hlsearch: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_state_default_is_zeroed() {
        let s = SearchState::default();
        assert!(!s.hl_match);
        assert_eq!(s.match_lines, 0);
        assert_eq!(s.cmdlen, 0);
        assert!(!s.no_hlsearch);
    }
}
