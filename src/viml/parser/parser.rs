//! Translated from `src/nvim/viml/parser/parser.c` and the inline
//! helpers in `src/nvim/viml/parser/parser.h`.

use crate::viml::parser::parser_defs::{
    ParserHighlight, ParserHighlightChunk, ParserInputReader,
    ParserLine, ParserLineGetter, ParserPosition, ParserState,
};

/// Cursor used by [`parser_simple_get_line`].
pub struct ParserLineCursor {
    /// Input lines, normally ending with `ParserLine::default()` EOF.
    pub lines: Vec<ParserLine>,
    /// Next line index.
    pub index: usize,
}

/// Initialize a parser state (`viml_parser_init`).
#[must_use]
pub fn viml_parser_init(
    get_line: ParserLineGetter,
    cookie: *mut std::ffi::c_void,
    colors: Option<*mut ParserHighlight>,
) -> ParserState {
    ParserState {
        reader: ParserInputReader {
            get_line,
            cookie,
            lines: Vec::new(),
            conv: crate::types_defs::VimconvT::default(),
        },
        pos: ParserPosition::default(),
        stack: Vec::new(),
        colors,
        can_continuate: false,
    }
}

/// Advance the parser position by `len` bytes
/// (`viml_parser_advance`).
pub fn viml_parser_advance(state: &mut ParserState, len: usize) {
    assert_eq!(state.pos.line, state.reader.lines.len() - 1);
    let line_size = state
        .reader
        .lines
        .last()
        .expect("parser reader has a current line")
        .size();
    if state.pos.col + len >= line_size {
        state.pos.line += 1;
        state.pos.col = 0;
    } else {
        state.pos.col += len;
    }
}

/// Record one highlighted source range (`viml_parser_highlight`).
///
/// # Safety
/// Any configured `colors` pointer must remain valid and exclusively
/// accessible for the parser state's lifetime.
pub unsafe fn viml_parser_highlight(
    state: &mut ParserState,
    start: ParserPosition,
    len: usize,
    group: &'static str,
) {
    let Some(colors) = state.colors else {
        return;
    };
    if len == 0 {
        return;
    }
    let colors = unsafe { &mut *colors };
    assert!(
        colors.last().is_none_or(|last| {
            last.start.line < start.line || last.end_col <= start.col
        })
    );
    colors.push(ParserHighlightChunk {
        start,
        end_col: start.col + len,
        group,
    });
}

/// Return successive lines from a [`ParserLineCursor`]
/// (`parser_simple_get_line`).
///
/// # Safety
/// `cookie` must point to a live, exclusively accessible
/// `ParserLineCursor`.
pub unsafe fn parser_simple_get_line(
    cookie: *mut std::ffi::c_void,
) -> ParserLine {
    let cursor = unsafe { &mut *cookie.cast::<ParserLineCursor>() };
    let line = cursor
        .lines
        .get(cursor.index)
        .cloned()
        .unwrap_or_default();
    cursor.index += 1;
    line
}

unsafe fn viml_preader_get_line(reader: &mut ParserInputReader) -> ParserLine {
    let line = unsafe { (reader.get_line)(reader.cookie) };
    if reader.conv.vc_type != crate::types_defs::ConvFlags::None
        && line.size() != 0
    {
        unimplemented!(
            "viml_preader_get_line scriptencoding conversion needs string_convert"
        );
    }
    reader.lines.push(line.clone());
    line
}

/// Get the current parser line shifted to `state.pos.col`
/// (`viml_parser_get_remaining_line`).
///
/// # Safety
/// Forwards the configured line getter's cookie contract.
#[must_use]
pub unsafe fn viml_parser_get_remaining_line(
    state: &mut ParserState,
) -> Option<ParserLine> {
    let mut line = if state.pos.line == state.reader.lines.len() {
        unsafe { viml_preader_get_line(&mut state.reader) }
    } else {
        state
            .reader
            .lines
            .last()
            .cloned()
            .expect("parser reader has a cached current line")
    };
    assert_eq!(state.pos.line, state.reader.lines.len() - 1);
    if let Some(data) = line.data.as_mut() {
        *data = data[state.pos.col..].to_vec();
        Some(line)
    } else {
        None
    }
}

/// Release parser-owned lines and stack state
/// (`viml_parser_destroy`).
///
/// Rust would perform the same cleanup automatically on drop; this
/// explicit function preserves the original's reusable-state API.
pub fn viml_parser_destroy(state: &mut ParserState) {
    state.reader.lines.clear();
    state.stack.clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::viml::parser::parser_defs::ParserStateItem;

    struct ParserFixture {
        cursor: *mut ParserLineCursor,
        state: ParserState,
    }

    impl ParserFixture {
        fn new(lines: &[&[u8]]) -> Self {
            let cursor = Box::into_raw(Box::new(ParserLineCursor {
                lines: lines
                    .iter()
                    .map(|line| ParserLine {
                        data: Some(line.to_vec()),
                    })
                    .chain(std::iter::once(ParserLine::default()))
                    .collect(),
                index: 0,
            }));
            let state = viml_parser_init(
                parser_simple_get_line,
                cursor.cast(),
                None,
            );
            Self { cursor, state }
        }
    }

    impl Drop for ParserFixture {
        fn drop(&mut self) {
            unsafe { drop(Box::from_raw(self.cursor)) };
        }
    }

    #[test]
    fn parser_simple_get_line_returns_lines_then_eof() {
        let mut fixture = ParserFixture::new(&[b"one", b"two"]);
        assert_eq!(
            unsafe { viml_parser_get_remaining_line(&mut fixture.state) },
            Some(ParserLine {
                data: Some(b"one".to_vec())
            })
        );
        fixture.state.pos.line = 1;
        assert_eq!(
            unsafe { viml_parser_get_remaining_line(&mut fixture.state) },
            Some(ParserLine {
                data: Some(b"two".to_vec())
            })
        );
        fixture.state.pos.line = 2;
        assert_eq!(
            unsafe { viml_parser_get_remaining_line(&mut fixture.state) },
            None
        );
        assert_eq!(unsafe { (*fixture.cursor).index }, 3);
    }

    #[test]
    fn remaining_line_starts_at_the_current_byte_column() {
        let mut fixture = ParserFixture::new(&[b"echo value"]);
        let _ = unsafe {
            viml_parser_get_remaining_line(&mut fixture.state)
        };
        fixture.state.pos.col = 5;
        assert_eq!(
            unsafe { viml_parser_get_remaining_line(&mut fixture.state) },
            Some(ParserLine {
                data: Some(b"value".to_vec())
            })
        );
        assert_eq!(unsafe { (*fixture.cursor).index }, 1);
    }

    #[test]
    fn parser_advance_stays_on_line_or_moves_to_the_next() {
        let mut fixture = ParserFixture::new(&[b"abc"]);
        let _ = unsafe {
            viml_parser_get_remaining_line(&mut fixture.state)
        };
        viml_parser_advance(&mut fixture.state, 2);
        assert_eq!(
            fixture.state.pos,
            ParserPosition { line: 0, col: 2 }
        );
        viml_parser_advance(&mut fixture.state, 1);
        assert_eq!(
            fixture.state.pos,
            ParserPosition { line: 1, col: 0 }
        );
    }

    #[test]
    fn parser_highlight_appends_ordered_nonempty_ranges() {
        let cursor = Box::into_raw(Box::new(ParserLineCursor {
            lines: vec![ParserLine::default()],
            index: 0,
        }));
        let mut colors = Vec::new();
        let colors_ptr = std::ptr::addr_of_mut!(colors);
        let mut state = viml_parser_init(
            parser_simple_get_line,
            cursor.cast(),
            Some(colors_ptr),
        );

        unsafe {
            viml_parser_highlight(
                &mut state,
                ParserPosition { line: 0, col: 1 },
                3,
                "Identifier",
            );
            viml_parser_highlight(
                &mut state,
                ParserPosition { line: 0, col: 4 },
                0,
                "Ignored",
            );
        }

        assert_eq!(
            unsafe { &*colors_ptr }.as_slice(),
            &[ParserHighlightChunk {
                start: ParserPosition { line: 0, col: 1 },
                end_col: 4,
                group: "Identifier",
            }]
        );
        unsafe { drop(Box::from_raw(cursor)) };
    }

    #[test]
    fn parser_destroy_clears_owned_vectors() {
        let mut fixture = ParserFixture::new(&[b"one"]);
        let _ = unsafe {
            viml_parser_get_remaining_line(&mut fixture.state)
        };
        fixture
            .state
            .stack
            .push(ParserStateItem::ParsingCommand);

        viml_parser_destroy(&mut fixture.state);

        assert!(fixture.state.reader.lines.is_empty());
        assert!(fixture.state.stack.is_empty());
    }

    #[test]
    #[should_panic(expected = "needs string_convert")]
    fn nondefault_scriptencoding_conversion_is_deferred() {
        let mut fixture = ParserFixture::new(&[b"one"]);
        fixture.state.reader.conv.vc_type =
            crate::types_defs::ConvFlags::ToUtf8;
        unsafe {
            let _ =
                viml_parser_get_remaining_line(&mut fixture.state);
        }
    }
}
