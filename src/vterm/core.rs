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

#[cfg(test)]
mod tests {
    use super::*;

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
}
