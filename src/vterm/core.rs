//! Translated from `src/nvim/vterm/vterm.c`.

/// Construction options (`VTermBuilder`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VTermBuilder {
    /// Reserved ABI version field.
    pub ver: i32,
    pub rows: i32,
    pub cols: i32,
    /// Zero selects the libvterm default of 4096.
    pub outbuffer_len: usize,
    /// Zero selects the libvterm default of 4096.
    pub tmpbuffer_len: usize,
}

/// Core terminal instance (`VTerm`).
#[allow(dead_code)]
#[derive(Debug)]
pub struct VTerm {
    rows: i32,
    cols: i32,
    utf8: bool,
    ctrl8bit: bool,
    pub parser: crate::vterm::parser::VTermParser,
    outbuffer: Vec<u8>,
    outbuffer_len: usize,
    tmpbuffer_len: usize,
}

/// Builds a terminal from explicit options (`vterm_build`).
#[must_use]
pub fn vterm_build(builder: &VTermBuilder) -> VTerm {
    let outbuffer_len = if builder.outbuffer_len == 0 {
        4096
    } else {
        builder.outbuffer_len
    };
    let tmpbuffer_len = if builder.tmpbuffer_len == 0 {
        4096
    } else {
        builder.tmpbuffer_len
    };
    VTerm {
        rows: builder.rows,
        cols: builder.cols,
        utf8: false,
        ctrl8bit: false,
        parser: crate::vterm::parser::VTermParser::default(),
        outbuffer: Vec::with_capacity(outbuffer_len),
        outbuffer_len,
        tmpbuffer_len,
    }
}

/// Builds a terminal with default allocator/buffer options (`vterm_new`).
#[must_use]
pub fn vterm_new(rows: i32, cols: i32) -> VTerm {
    vterm_build(&VTermBuilder {
        rows,
        cols,
        ..Default::default()
    })
}

/// Writes the current dimensions to requested outputs (`vterm_get_size`).
pub fn vterm_get_size(
    term: &VTerm,
    rows: Option<&mut i32>,
    cols: Option<&mut i32>,
) {
    if let Some(rows) = rows {
        *rows = term.rows;
    }
    if let Some(cols) = cols {
        *cols = term.cols;
    }
}

/// Updates terminal dimensions and notifies the parser callback
/// (`vterm_set_size`).
pub fn vterm_set_size(
    term: &mut VTerm,
    rows: i32,
    cols: i32,
    callbacks: Option<&mut dyn crate::vterm::parser::VTermParserCallbacks>,
) {
    if rows < 1 || cols < 1 {
        return;
    }
    term.rows = rows;
    term.cols = cols;
    if let Some(callbacks) = callbacks {
        let _ = callbacks.resize(rows, cols);
    }
}

/// Selects whether input bytes are interpreted as UTF-8
/// (`vterm_set_utf8`).
pub fn vterm_set_utf8(term: &mut VTerm, is_utf8: i32) {
    term.utf8 = is_utf8 != 0;
}

/// Parses input using this terminal's current mode
/// (`vterm_input_write`).
pub fn vterm_input_write<C: crate::vterm::parser::VTermParserCallbacks>(
    term: &mut VTerm,
    callbacks: &mut C,
    bytes: &[u8],
) -> usize {
    term.parser
        .vterm_input_write(callbacks, bytes, term.utf8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct ResizeCapture(Vec<(i32, i32)>);

    impl crate::vterm::parser::VTermParserCallbacks for ResizeCapture {
        fn resize(&mut self, rows: i32, cols: i32) -> bool {
            self.0.push((rows, cols));
            true
        }
    }

    #[test]
    fn vterm_builder_defaults_match_a_zeroed_c_builder() {
        assert_eq!(VTermBuilder::default(), VTermBuilder {
            ver: 0,
            rows: 0,
            cols: 0,
            outbuffer_len: 0,
            tmpbuffer_len: 0,
        });
    }

    #[test]
    fn vterm_build_initializes_core_fields_and_default_buffers() {
        let term = vterm_build(&VTermBuilder {
            rows: 24,
            cols: 80,
            ..Default::default()
        });
        assert_eq!((term.rows, term.cols), (24, 80));
        assert!(!term.utf8);
        assert!(!term.ctrl8bit);
        assert_eq!(
            term.parser.state,
            crate::vterm::parser::VTermParserState::Normal
        );
        assert!(!term.parser.emit_nul);
        assert_eq!(term.outbuffer_len, 4096);
        assert_eq!(term.outbuffer.capacity(), 4096);
        assert!(term.outbuffer.is_empty());
        assert_eq!(term.tmpbuffer_len, 4096);
    }

    #[test]
    fn vterm_new_and_builder_honor_explicit_dimensions_and_buffer_sizes() {
        let term = vterm_new(12, 34);
        assert_eq!((term.rows, term.cols), (12, 34));

        let term = vterm_build(&VTermBuilder {
            rows: 5,
            cols: 6,
            outbuffer_len: 17,
            tmpbuffer_len: 19,
            ..Default::default()
        });
        assert_eq!((term.rows, term.cols), (5, 6));
        assert_eq!(term.outbuffer_len, 17);
        assert_eq!(term.outbuffer.capacity(), 17);
        assert_eq!(term.tmpbuffer_len, 19);
    }

    #[test]
    fn vterm_get_size_writes_only_requested_outputs() {
        let term = vterm_new(24, 80);
        let mut rows = -1;
        let mut cols = -1;
        vterm_get_size(&term, Some(&mut rows), Some(&mut cols));
        assert_eq!((rows, cols), (24, 80));

        rows = -1;
        cols = -1;
        vterm_get_size(&term, Some(&mut rows), None);
        assert_eq!((rows, cols), (24, -1));
        vterm_get_size(&term, None, Some(&mut cols));
        assert_eq!((rows, cols), (24, 80));
    }

    #[test]
    fn vterm_set_size_updates_valid_dimensions_and_calls_resize() {
        let mut term = vterm_new(24, 80);
        let mut capture = ResizeCapture::default();
        vterm_set_size(&mut term, 40, 120, Some(&mut capture));
        let mut rows = 0;
        let mut cols = 0;
        vterm_get_size(&term, Some(&mut rows), Some(&mut cols));
        assert_eq!((rows, cols), (40, 120));
        assert_eq!(capture.0, [(40, 120)]);
    }

    #[test]
    fn vterm_set_size_rejects_nonpositive_dimensions() {
        let mut term = vterm_new(24, 80);
        let mut capture = ResizeCapture::default();
        for (rows, cols) in [(0, 80), (-1, 80), (24, 0), (24, -1)] {
            vterm_set_size(&mut term, rows, cols, Some(&mut capture));
        }
        let mut rows = 0;
        let mut cols = 0;
        vterm_get_size(&term, Some(&mut rows), Some(&mut cols));
        assert_eq!((rows, cols), (24, 80));
        assert!(capture.0.is_empty());
    }

    #[test]
    fn vterm_set_utf8_uses_c_boolean_semantics() {
        let mut term = vterm_new(24, 80);
        assert!(!term.utf8);
        vterm_set_utf8(&mut term, 1);
        assert!(term.utf8);
        vterm_set_utf8(&mut term, -1);
        assert!(term.utf8);
        vterm_set_utf8(&mut term, 0);
        assert!(!term.utf8);
    }

    #[derive(Default)]
    struct InputCapture {
        texts: Vec<Vec<u8>>,
        controls: Vec<u8>,
    }

    impl crate::vterm::parser::VTermParserCallbacks for InputCapture {
        fn text(&mut self, bytes: &[u8]) -> usize {
            self.texts.push(bytes.to_vec());
            bytes.len()
        }

        fn control(&mut self, control: u8) -> bool {
            self.controls.push(control);
            true
        }
    }

    #[test]
    fn vterm_input_write_uses_the_terminal_utf8_mode() {
        let mut term = vterm_new(24, 80);
        let mut capture = InputCapture::default();

        // In non-UTF-8 mode C1 bytes are controls.
        vterm_input_write(&mut term, &mut capture, b"\x84");
        assert_eq!(capture.controls, [0x84]);
        assert!(capture.texts.is_empty());

        vterm_set_utf8(&mut term, 1);
        capture.controls.clear();
        vterm_input_write(&mut term, &mut capture, b"\x84");
        assert!(capture.controls.is_empty());
        assert_eq!(capture.texts, [vec![0x84]]);
        assert_eq!(
            term.parser.state,
            crate::vterm::parser::VTermParserState::Normal
        );
    }
}
