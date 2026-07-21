//! Translated from `src/nvim/fold_defs.h`.

use crate::pos_defs::LinenrT;

/// Info used to pass info about a fold from the fold-detection code to the
/// code that displays the foldcolumn (`foldinfo_T`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FoldinfoT {
    /// line number where fold starts
    pub fi_lnum: LinenrT,
    /// level of the fold; when this is zero the other fields are invalid
    pub fi_level: i32,
    /// lowest fold level that starts in the same line
    pub fi_low_level: i32,
    pub fi_lines: LinenrT,
}

/// buffer size for `get_foldtext()` (`FOLD_TEXT_LEN`).
pub const FOLD_TEXT_LEN: usize = 51;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foldinfo_default_is_invalid_level() {
        // fi_level == 0 means "the other fields are invalid" per the
        // original's doc comment; Default::default() must produce that.
        let fi = FoldinfoT::default();
        assert_eq!(fi.fi_level, 0);
        assert_eq!(fi.fi_lnum, 0);
    }
}
