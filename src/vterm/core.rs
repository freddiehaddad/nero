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
pub struct VTerm {
    rows: i32,
    cols: i32,
    utf8: bool,
    ctrl8bit: bool,
    pub parser: crate::vterm::parser::VTermParser,
    outbuffer: Vec<u8>,
    outbuffer_len: usize,
    tmpbuffer_len: usize,
    outfunc: Option<VTermOutputCallback>,
}

/// Output callback plus its captured user data (`VTermOutputCallback`).
pub type VTermOutputCallback = Box<dyn FnMut(&[u8])>;

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
        outfunc: None,
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

/// Appends bytes to the internal output buffer
/// (`vterm_push_output_bytes`, buffered path).
pub fn vterm_push_output_bytes(term: &mut VTerm, bytes: &[u8]) {
    if let Some(callback) = term.outfunc.as_mut() {
        callback(bytes);
        return;
    }
    if bytes.len() > term.outbuffer_len - term.outbuffer.len() {
        return;
    }
    term.outbuffer.extend_from_slice(bytes);
}

/// Installs or removes the output callback
/// (`vterm_output_set_callback`).
pub fn vterm_output_set_callback(
    term: &mut VTerm,
    callback: Option<VTermOutputCallback>,
) {
    term.outfunc = callback;
}

/// Formats and emits output (`vterm_push_output_sprintf`).
pub fn vterm_push_output_sprintf(
    term: &mut VTerm,
    arguments: std::fmt::Arguments<'_>,
) {
    let formatted = arguments.to_string();
    assert!(
        formatted.len() < term.tmpbuffer_len,
        "formatted output exceeds VTerm tmpbuffer"
    );
    vterm_push_output_bytes(term, formatted.as_bytes());
}

/// Emits a control byte plus formatted payload
/// (`vterm_push_output_sprintf_ctrl`).
pub fn vterm_push_output_sprintf_ctrl(
    term: &mut VTerm,
    control: u8,
    arguments: std::fmt::Arguments<'_>,
) {
    let mut output = Vec::new();
    if control >= 0x80 && !term.ctrl8bit {
        output.extend_from_slice(&[0x1B, control - 0x40]);
    } else {
        output.push(control);
    }
    if output.len() >= term.tmpbuffer_len {
        return;
    }
    output.extend_from_slice(arguments.to_string().as_bytes());
    if output.len() >= term.tmpbuffer_len {
        return;
    }
    vterm_push_output_bytes(term, &output);
}

/// Emits an optional control, formatted string payload, and optional
/// string terminator (`vterm_push_output_sprintf_str`).
pub fn vterm_push_output_sprintf_str(
    term: &mut VTerm,
    control: u8,
    terminate: bool,
    arguments: std::fmt::Arguments<'_>,
) {
    let mut output = Vec::new();
    if control != 0 {
        if control >= 0x80 && !term.ctrl8bit {
            output.extend_from_slice(&[0x1B, control - 0x40]);
        } else {
            output.push(control);
        }
        if output.len() >= term.tmpbuffer_len {
            return;
        }
    }

    output.extend_from_slice(arguments.to_string().as_bytes());
    if output.len() >= term.tmpbuffer_len {
        return;
    }

    if terminate {
        if term.ctrl8bit {
            output.push(crate::vterm_defs::C1_ST);
        } else {
            output.extend_from_slice(b"\x1b\\");
        }
        if output.len() >= term.tmpbuffer_len {
            return;
        }
    }
    vterm_push_output_bytes(term, &output);
}

/// Returns the value kind required by a pen attribute
/// (`vterm_get_attr_type`).
#[must_use]
pub const fn vterm_get_attr_type(
    attr: crate::vterm_defs::VTermAttr,
) -> crate::vterm_defs::VTermValueType {
    use crate::vterm_defs::{VTermAttr as Attr, VTermValueType as Type};
    match attr {
        Attr::Bold
        | Attr::Italic
        | Attr::Blink
        | Attr::Reverse
        | Attr::Conceal
        | Attr::Strike
        | Attr::Small
        | Attr::Dim
        | Attr::Overline => Type::Bool,
        Attr::Underline | Attr::Font | Attr::Baseline | Attr::Uri => Type::Int,
        Attr::Foreground | Attr::Background => Type::Color,
        Attr::None | Attr::NAttrs => Type::None,
    }
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

    #[test]
    fn push_output_bytes_appends_when_the_entire_write_fits() {
        let mut term = vterm_build(&VTermBuilder {
            outbuffer_len: 6,
            ..Default::default()
        });
        vterm_push_output_bytes(&mut term, b"ab");
        vterm_push_output_bytes(&mut term, b"cdef");
        assert_eq!(term.outbuffer, b"abcdef");
    }

    #[test]
    fn push_output_bytes_drops_an_entire_overflowing_write() {
        let mut term = vterm_build(&VTermBuilder {
            outbuffer_len: 5,
            ..Default::default()
        });
        vterm_push_output_bytes(&mut term, b"abc");
        vterm_push_output_bytes(&mut term, b"def");
        assert_eq!(term.outbuffer, b"abc");
        vterm_push_output_bytes(&mut term, b"de");
        assert_eq!(term.outbuffer, b"abcde");
    }

    #[test]
    fn output_callback_overrides_internal_buffering() {
        let mut term = vterm_build(&VTermBuilder {
            outbuffer_len: 1,
            ..Default::default()
        });
        let captured = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let callback_capture = std::rc::Rc::clone(&captured);
        vterm_output_set_callback(
            &mut term,
            Some(Box::new(move |bytes| {
                callback_capture.borrow_mut().push(bytes.to_vec());
            })),
        );
        vterm_push_output_bytes(&mut term, b"long");
        vterm_push_output_bytes(&mut term, b"writes");
        assert_eq!(&*captured.borrow(), &[b"long".to_vec(), b"writes".to_vec()]);
        assert!(term.outbuffer.is_empty());

        vterm_output_set_callback(&mut term, None);
        vterm_push_output_bytes(&mut term, b"x");
        assert_eq!(term.outbuffer, b"x");
    }

    #[test]
    fn push_output_sprintf_formats_into_the_output_path() {
        let mut term = vterm_new(24, 80);
        vterm_push_output_sprintf(&mut term, format_args!("{};{}{}", 12, 3, 'm'));
        assert_eq!(term.outbuffer, b"12;3m");

        let captured = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let callback_capture = std::rc::Rc::clone(&captured);
        vterm_output_set_callback(
            &mut term,
            Some(Box::new(move |bytes| {
                callback_capture.borrow_mut().extend_from_slice(bytes);
            })),
        );
        vterm_push_output_sprintf(&mut term, format_args!("{}x{}", 4, 5));
        assert_eq!(&*captured.borrow(), b"4x5");
    }

    #[test]
    #[should_panic(expected = "formatted output exceeds VTerm tmpbuffer")]
    fn push_output_sprintf_enforces_the_c_tmpbuffer_contract() {
        let mut term = vterm_build(&VTermBuilder {
            tmpbuffer_len: 4,
            ..Default::default()
        });
        vterm_push_output_sprintf(&mut term, format_args!("abcd"));
    }

    #[test]
    fn push_output_sprintf_ctrl_selects_seven_or_eight_bit_control() {
        let mut term = vterm_new(24, 80);
        vterm_push_output_sprintf_ctrl(
            &mut term,
            crate::vterm_defs::C1_CSI,
            format_args!("{}m", 4),
        );
        assert_eq!(term.outbuffer, b"\x1b[4m");

        term.outbuffer.clear();
        term.ctrl8bit = true;
        vterm_push_output_sprintf_ctrl(
            &mut term,
            crate::vterm_defs::C1_CSI,
            format_args!("{}m", 4),
        );
        assert_eq!(
            term.outbuffer,
            [vec![crate::vterm_defs::C1_CSI], b"4m".to_vec()].concat()
        );
    }

    #[test]
    fn push_output_sprintf_ctrl_drops_overlong_output() {
        let mut term = vterm_build(&VTermBuilder {
            tmpbuffer_len: 5,
            ..Default::default()
        });
        vterm_push_output_sprintf_ctrl(
            &mut term,
            crate::vterm_defs::C1_CSI,
            format_args!("123"),
        );
        assert!(term.outbuffer.is_empty());

        vterm_push_output_sprintf_ctrl(&mut term, b'\r', format_args!("12"));
        assert_eq!(term.outbuffer, b"\r12");
    }

    #[test]
    fn push_output_sprintf_str_handles_optional_control_and_terminator() {
        let mut term = vterm_new(24, 80);
        vterm_push_output_sprintf_str(
            &mut term,
            crate::vterm_defs::C1_OSC,
            true,
            format_args!("{};{}", 2, "title"),
        );
        assert_eq!(term.outbuffer, b"\x1b]2;title\x1b\\");

        term.outbuffer.clear();
        vterm_push_output_sprintf_str(
            &mut term,
            0,
            false,
            format_args!("plain"),
        );
        assert_eq!(term.outbuffer, b"plain");
    }

    #[test]
    fn push_output_sprintf_str_uses_eight_bit_control_and_st() {
        let mut term = vterm_new(24, 80);
        term.ctrl8bit = true;
        vterm_push_output_sprintf_str(
            &mut term,
            crate::vterm_defs::C1_DCS,
            true,
            format_args!("1$r"),
        );
        assert_eq!(
            term.outbuffer,
            [
                vec![crate::vterm_defs::C1_DCS],
                b"1$r".to_vec(),
                vec![crate::vterm_defs::C1_ST],
            ]
            .concat()
        );
    }

    #[test]
    fn push_output_sprintf_str_drops_any_stage_that_fills_tmpbuffer() {
        let mut term = vterm_build(&VTermBuilder {
            tmpbuffer_len: 6,
            ..Default::default()
        });
        vterm_push_output_sprintf_str(
            &mut term,
            crate::vterm_defs::C1_OSC,
            false,
            format_args!("1234"),
        );
        assert!(term.outbuffer.is_empty());

        vterm_push_output_sprintf_str(&mut term, 0, true, format_args!("1234"));
        assert!(term.outbuffer.is_empty());
    }

    #[test]
    fn get_attr_type_matches_every_attribute_case() {
        use crate::vterm_defs::{VTermAttr as Attr, VTermValueType as Type};
        for attr in [
            Attr::Bold,
            Attr::Italic,
            Attr::Blink,
            Attr::Reverse,
            Attr::Conceal,
            Attr::Strike,
            Attr::Small,
            Attr::Dim,
            Attr::Overline,
        ] {
            assert_eq!(vterm_get_attr_type(attr), Type::Bool, "{attr:?}");
        }
        for attr in [Attr::Underline, Attr::Font, Attr::Baseline, Attr::Uri] {
            assert_eq!(vterm_get_attr_type(attr), Type::Int, "{attr:?}");
        }
        for attr in [Attr::Foreground, Attr::Background] {
            assert_eq!(vterm_get_attr_type(attr), Type::Color, "{attr:?}");
        }
        assert_eq!(vterm_get_attr_type(Attr::None), Type::None);
        assert_eq!(vterm_get_attr_type(Attr::NAttrs), Type::None);
    }
}
