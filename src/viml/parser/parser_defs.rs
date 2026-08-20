//! Translated from `src/nvim/viml/parser/parser_defs.h` in full.

/// One parsed line (`ParserLine`).
///
/// The original's pointer/size/allocated triple becomes one owned
/// optional byte vector. `None` is EOF; Rust ownership removes the
/// separate "may be freed" flag.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParserLine {
    /// Parsed line bytes, or `None` for EOF.
    pub data: Option<Vec<u8>>,
}

impl ParserLine {
    /// Parsed line byte count.
    #[must_use]
    pub fn size(&self) -> usize {
        self.data.as_ref().map_or(0, Vec::len)
    }
}

/// Line getter callback (`ParserLineGetter`).
///
/// # Safety
/// `cookie` must satisfy the selected callback's contract.
pub type ParserLineGetter =
    unsafe fn(cookie: *mut std::ffi::c_void) -> ParserLine;

/// Parser position in the input (`ParserPosition`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ParserPosition {
    /// Line index in `ParserInputReader::lines`.
    pub line: usize,
    /// Byte index in the line.
    pub col: usize,
}

/// One parser-state stack item (`ParserStateItem`).
///
/// The original's tag-plus-union currently has only a command state
/// and one expression placeholder; a Rust enum represents the same
/// states without an invalid tag/union combination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParserStateItem {
    /// `kPTopStateParsingCommand`.
    ParsingCommand,
    /// `kPTopStateParsingExpression` with `kExprUnknown`.
    ParsingExpressionUnknown,
}

/// Input reader state (`ParserInputReader`).
pub struct ParserInputReader {
    /// Function used to obtain the next line.
    pub get_line: ParserLineGetter,
    /// Opaque callback argument.
    pub cookie: *mut std::ffi::c_void,
    /// Every line obtained from `get_line`.
    pub lines: Vec<ParserLine>,
    /// `:scriptencoding` conversion state.
    pub conv: crate::types_defs::VimconvT,
}

/// One highlighted source range (`ParserHighlightChunk`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParserHighlightChunk {
    /// Highlight start position.
    pub start: ParserPosition,
    /// Exclusive end byte column on the same line.
    pub end_col: usize,
    /// Highlight-group name.
    pub group: &'static str,
}

/// Highlighting produced by a parser (`ParserHighlight`).
pub type ParserHighlight = Vec<ParserHighlightChunk>;

/// Complete parser state (`ParserState`).
pub struct ParserState {
    /// Line reader.
    pub reader: ParserInputReader,
    /// Position parsed through.
    pub pos: ParserPosition,
    /// Parser-state stack.
    pub stack: Vec<ParserStateItem>,
    /// Optional caller-owned highlight output.
    pub colors: Option<*mut ParserHighlight>,
    /// Whether line continuation may be used.
    pub can_continuate: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_line_none_is_eof_and_some_tracks_its_size() {
        assert_eq!(ParserLine::default().size(), 0);
        let line = ParserLine {
            data: Some(b"echo 1".to_vec()),
        };
        assert_eq!(line.size(), 6);
    }

    #[test]
    fn parser_position_default_starts_at_the_first_byte() {
        assert_eq!(ParserPosition::default(), ParserPosition { line: 0, col: 0 });
    }

    #[test]
    fn parser_state_item_variants_preserve_both_top_level_states() {
        assert_ne!(
            ParserStateItem::ParsingCommand,
            ParserStateItem::ParsingExpressionUnknown
        );
    }
}
