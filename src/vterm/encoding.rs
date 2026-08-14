//! Translated from `src/nvim/vterm/encoding.c`.
//!
//! This module starts with the state carried across calls to the UTF-8
//! decoder and its initialization routine. The decoder and the other
//! libvterm character sets, their lookup table, and their dispatch
//! methods are translated incrementally below.

const UNICODE_INVALID: u32 = 0xFFFD;

/// Encoding class (`VTermEncodingType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum VTermEncodingType {
    Utf8 = 0,
    Single94 = 1,
}

/// One entry from libvterm's static encoding table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VTermEncoding {
    Utf8,
    DecSpecialGraphics,
    UsAscii,
}

/// One selected encoding and its persistent decoder state
/// (`VTermEncodingInstance`).
#[derive(Debug, Clone)]
pub struct VTermEncodingInstance {
    encoding: VTermEncoding,
    utf8_data: Utf8DecoderData,
}

impl VTermEncodingInstance {
    /// Selects and initializes `encoding`.
    #[must_use]
    pub fn new(encoding: VTermEncoding) -> Self {
        let mut instance = Self {
            encoding,
            utf8_data: Utf8DecoderData::default(),
        };
        instance.reset();
        instance
    }

    /// Returns the selected static encoding.
    #[must_use]
    pub fn encoding(&self) -> VTermEncoding {
        self.encoding
    }

    /// Invokes the encoding's optional initialization callback.
    pub fn reset(&mut self) {
        if self.encoding == VTermEncoding::Utf8 {
            init_utf8(&mut self.utf8_data);
        }
    }

    /// Invokes the selected encoding's decode callback.
    pub fn decode(
        &mut self,
        codepoints: &mut [u32],
        cpi: &mut usize,
        bytes: &[u8],
        pos: &mut usize,
    ) {
        match self.encoding {
            VTermEncoding::Utf8 => {
                decode_utf8(&mut self.utf8_data, codepoints, cpi, bytes, pos);
            }
            VTermEncoding::DecSpecialGraphics => {
                decode_table(&DEC_SPECIAL_GRAPHICS, codepoints, cpi, bytes, pos);
            }
            VTermEncoding::UsAscii => decode_usascii(codepoints, cpi, bytes, pos),
        }
    }
}

/// Stateful UTF-8 decoder storage (`struct UTF8DecoderData`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Utf8DecoderData {
    /// Number of bytes still required by the current codepoint.
    bytes_remaining: i32,
    /// Total encoded length, used to detect overlong sequences.
    bytes_total: i32,
    /// Codepoint accumulated so far.
    this_cp: i32,
}

/// Initializes a UTF-8 decoder (`init_utf8`).
#[allow(dead_code)]
fn init_utf8(data: &mut Utf8DecoderData) {
    data.bytes_remaining = 0;
    data.bytes_total = 0;
}

/// Decodes UTF-8 bytes while preserving incomplete state across calls
/// (`decode_utf8`).
///
/// `codepoints` must have room for the worst-case output of the input:
/// an interrupted sequence can emit a replacement character followed
/// by the new character in one iteration. The original has the same
/// caller-side capacity requirement.
#[allow(dead_code)]
fn decode_utf8(
    data: &mut Utf8DecoderData,
    codepoints: &mut [u32],
    cpi: &mut usize,
    bytes: &[u8],
    pos: &mut usize,
) {
    while *pos < bytes.len() && *cpi < codepoints.len() {
        let c = bytes[*pos];

        if c < 0x20 {
            return;
        } else if c < 0x7F {
            if data.bytes_remaining != 0 {
                codepoints[*cpi] = UNICODE_INVALID;
                *cpi += 1;
            }

            codepoints[*cpi] = u32::from(c);
            *cpi += 1;
            data.bytes_remaining = 0;
        } else if c == 0x7F {
            return;
        } else if c < 0xC0 {
            if data.bytes_remaining == 0 {
                codepoints[*cpi] = UNICODE_INVALID;
                *cpi += 1;
                *pos += 1;
                continue;
            }

            data.this_cp = (data.this_cp << 6) | i32::from(c & 0x3F);
            data.bytes_remaining -= 1;

            if data.bytes_remaining == 0 {
                match data.bytes_total {
                    2 if data.this_cp < 0x0080 => data.this_cp = UNICODE_INVALID as i32,
                    3 if data.this_cp < 0x0800 => data.this_cp = UNICODE_INVALID as i32,
                    4 if data.this_cp < 0x10000 => data.this_cp = UNICODE_INVALID as i32,
                    5 if data.this_cp < 0x200000 => data.this_cp = UNICODE_INVALID as i32,
                    6 if data.this_cp < 0x4000000 => data.this_cp = UNICODE_INVALID as i32,
                    _ => {}
                }
                if (0xD800..=0xDFFF).contains(&data.this_cp)
                    || data.this_cp == 0xFFFE
                    || data.this_cp == 0xFFFF
                {
                    data.this_cp = UNICODE_INVALID as i32;
                }
                codepoints[*cpi] = data.this_cp as u32;
                *cpi += 1;
            }
        } else if c < 0xE0 {
            if data.bytes_remaining != 0 {
                codepoints[*cpi] = UNICODE_INVALID;
                *cpi += 1;
            }
            data.this_cp = i32::from(c & 0x1F);
            data.bytes_total = 2;
            data.bytes_remaining = 1;
        } else if c < 0xF0 {
            if data.bytes_remaining != 0 {
                codepoints[*cpi] = UNICODE_INVALID;
                *cpi += 1;
            }
            data.this_cp = i32::from(c & 0x0F);
            data.bytes_total = 3;
            data.bytes_remaining = 2;
        } else if c < 0xF8 {
            if data.bytes_remaining != 0 {
                codepoints[*cpi] = UNICODE_INVALID;
                *cpi += 1;
            }
            data.this_cp = i32::from(c & 0x07);
            data.bytes_total = 4;
            data.bytes_remaining = 3;
        } else if c < 0xFC {
            if data.bytes_remaining != 0 {
                codepoints[*cpi] = UNICODE_INVALID;
                *cpi += 1;
            }
            data.this_cp = i32::from(c & 0x03);
            data.bytes_total = 5;
            data.bytes_remaining = 4;
        } else if c < 0xFE {
            if data.bytes_remaining != 0 {
                codepoints[*cpi] = UNICODE_INVALID;
                *cpi += 1;
            }
            data.this_cp = i32::from(c & 0x01);
            data.bytes_total = 6;
            data.bytes_remaining = 5;
        } else {
            codepoints[*cpi] = UNICODE_INVALID;
            *cpi += 1;
        }

        *pos += 1;
    }
}

/// Decodes libvterm's US-ASCII GL or GR character set
/// (`decode_usascii`).
///
/// `*pos` must identify a byte in `bytes`, matching the original's
/// caller contract: that byte selects whether this invocation decodes
/// the low (GL) or high (GR) half.
#[allow(dead_code)]
fn decode_usascii(
    codepoints: &mut [u32],
    cpi: &mut usize,
    bytes: &[u8],
    pos: &mut usize,
) {
    let is_gr = bytes[*pos] & 0x80;
    while *pos < bytes.len() && *cpi < codepoints.len() {
        let c = bytes[*pos] ^ is_gr;
        if c < 0x20 || c == 0x7F || c >= 0x80 {
            return;
        }

        codepoints[*cpi] = u32::from(c);
        *cpi += 1;
        *pos += 1;
    }
}

/// DEC Special Graphics character map (`encoding_DECdrawing.chars`).
#[allow(dead_code)]
const DEC_SPECIAL_GRAPHICS: [u32; 128] = {
    let mut chars = [0; 128];
    chars[0x60] = 0x25C6; // BLACK DIAMOND
    chars[0x61] = 0x2592; // MEDIUM SHADE
    chars[0x62] = 0x2409; // SYMBOL FOR HORIZONTAL TAB
    chars[0x63] = 0x240C; // SYMBOL FOR FORM FEED
    chars[0x64] = 0x240D; // SYMBOL FOR CARRIAGE RETURN
    chars[0x65] = 0x240A; // SYMBOL FOR LINE FEED
    chars[0x66] = 0x00B0; // DEGREE SIGN
    chars[0x67] = 0x00B1; // PLUS-MINUS SIGN
    chars[0x68] = 0x2424; // SYMBOL FOR NEW LINE
    chars[0x69] = 0x240B; // SYMBOL FOR VERTICAL TAB
    chars[0x6A] = 0x2518; // BOX DRAWINGS LIGHT UP AND LEFT
    chars[0x6B] = 0x2510; // BOX DRAWINGS LIGHT DOWN AND LEFT
    chars[0x6C] = 0x250C; // BOX DRAWINGS LIGHT DOWN AND RIGHT
    chars[0x6D] = 0x2514; // BOX DRAWINGS LIGHT UP AND RIGHT
    chars[0x6E] = 0x253C; // BOX DRAWINGS LIGHT VERTICAL AND HORIZONTAL
    chars[0x6F] = 0x23BA; // HORIZONTAL SCAN LINE-1
    chars[0x70] = 0x23BB; // HORIZONTAL SCAN LINE-3
    chars[0x71] = 0x2500; // BOX DRAWINGS LIGHT HORIZONTAL
    chars[0x72] = 0x23BC; // HORIZONTAL SCAN LINE-7
    chars[0x73] = 0x23BD; // HORIZONTAL SCAN LINE-9
    chars[0x74] = 0x251C; // BOX DRAWINGS LIGHT VERTICAL AND RIGHT
    chars[0x75] = 0x2524; // BOX DRAWINGS LIGHT VERTICAL AND LEFT
    chars[0x76] = 0x2534; // BOX DRAWINGS LIGHT UP AND HORIZONTAL
    chars[0x77] = 0x252C; // BOX DRAWINGS LIGHT DOWN AND HORIZONTAL
    chars[0x78] = 0x2502; // BOX DRAWINGS LIGHT VERTICAL
    chars[0x79] = 0x2A7D; // LESS-THAN OR SLANTED EQUAL-TO
    chars[0x7A] = 0x2A7E; // GREATER-THAN OR SLANTED EQUAL-TO
    chars[0x7B] = 0x03C0; // GREEK SMALL LETTER PI
    chars[0x7C] = 0x2260; // NOT EQUAL TO
    chars[0x7D] = 0x00A3; // POUND SIGN
    chars[0x7E] = 0x00B7; // MIDDLE DOT
    chars
};

/// Decodes a fixed libvterm single-byte table (`decode_table`).
#[allow(dead_code)]
fn decode_table(
    table: &[u32; 128],
    codepoints: &mut [u32],
    cpi: &mut usize,
    bytes: &[u8],
    pos: &mut usize,
) {
    let is_gr = bytes[*pos] & 0x80;
    while *pos < bytes.len() && *cpi < codepoints.len() {
        let c = bytes[*pos] ^ is_gr;
        if c < 0x20 || c == 0x7F || c >= 0x80 {
            return;
        }

        let mapped = table[usize::from(c)];
        codepoints[*cpi] = if mapped != 0 { mapped } else { u32::from(c) };
        *cpi += 1;
        *pos += 1;
    }
}

/// Looks up an encoding by class and designation
/// (`vterm_lookup_encoding`).
#[must_use]
pub fn vterm_lookup_encoding(
    encoding_type: VTermEncodingType,
    designation: u8,
) -> Option<VTermEncoding> {
    match (encoding_type, designation) {
        (VTermEncodingType::Utf8, b'u') => Some(VTermEncoding::Utf8),
        (VTermEncodingType::Single94, b'0') => Some(VTermEncoding::DecSpecialGraphics),
        (VTermEncodingType::Single94, b'B') => Some(VTermEncoding::UsAscii),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_utf8_resets_sequence_lengths_but_not_scratch_codepoint() {
        let mut data = Utf8DecoderData {
            bytes_remaining: 3,
            bytes_total: 4,
            this_cp: 0x1234,
        };
        init_utf8(&mut data);
        assert_eq!(data.bytes_remaining, 0);
        assert_eq!(data.bytes_total, 0);
        // The original deliberately leaves this scratch field alone.
        assert_eq!(data.this_cp, 0x1234);
    }

    fn decode(bytes: &[u8]) -> (Vec<u32>, Utf8DecoderData, usize) {
        let mut data = Utf8DecoderData::default();
        init_utf8(&mut data);
        let mut output = [0; 32];
        let mut cpi = 0;
        let mut pos = 0;
        decode_utf8(&mut data, &mut output, &mut cpi, bytes, &mut pos);
        (output[..cpi].to_vec(), data, pos)
    }

    #[test]
    fn decode_utf8_decodes_ascii_and_multibyte_sequences() {
        let (output, data, pos) = decode(b"A\xC2\xA3\xE2\x82\xAC\xF0\x9F\x98\x80");
        assert_eq!(output, [0x41, 0x00A3, 0x20AC, 0x1F600]);
        assert_eq!(data.bytes_remaining, 0);
        assert_eq!(pos, 10);
    }

    #[test]
    fn decode_utf8_stops_before_c0_and_del_without_consuming_them() {
        assert_eq!(decode(b"ab\x1bcd").0, [b'a' as u32, b'b' as u32]);
        assert_eq!(decode(b"ab\x1bcd").2, 2);
        assert_eq!(decode(b"ab\x7fcd").0, [b'a' as u32, b'b' as u32]);
        assert_eq!(decode(b"ab\x7fcd").2, 2);
    }

    #[test]
    fn decode_utf8_preserves_an_incomplete_sequence_across_calls() {
        let mut data = Utf8DecoderData::default();
        let mut output = [0; 4];
        let mut cpi = 0;
        let mut pos = 0;
        decode_utf8(&mut data, &mut output, &mut cpi, b"\xE2\x82", &mut pos);
        assert_eq!(cpi, 0);
        assert_eq!(data.bytes_remaining, 1);

        pos = 0;
        decode_utf8(&mut data, &mut output, &mut cpi, b"\xAC", &mut pos);
        assert_eq!(&output[..cpi], &[0x20AC]);
        assert_eq!(data.bytes_remaining, 0);
    }

    #[test]
    fn decode_utf8_replaces_unexpected_or_interrupted_sequences() {
        assert_eq!(decode(b"\x80A").0, [UNICODE_INVALID, b'A' as u32]);
        assert_eq!(
            decode(b"\xE2A").0,
            [UNICODE_INVALID, b'A' as u32]
        );
        assert_eq!(
            decode(b"\xE2\xC2\xA3").0,
            [UNICODE_INVALID, 0x00A3]
        );
    }

    #[test]
    fn decode_utf8_rejects_overlong_and_plain_invalid_codepoints() {
        assert_eq!(decode(b"\xC0\x80").0, [UNICODE_INVALID]);
        assert_eq!(decode(b"\xE0\x80\x80").0, [UNICODE_INVALID]);
        assert_eq!(decode(b"\xF0\x80\x80\x80").0, [UNICODE_INVALID]);
        assert_eq!(decode(b"\xED\xA0\x80").0, [UNICODE_INVALID]);
        assert_eq!(decode(b"\xEF\xBF\xBE").0, [UNICODE_INVALID]);
        assert_eq!(decode(b"\xEF\xBF\xBF").0, [UNICODE_INVALID]);
        assert_eq!(decode(b"\xFE\xFF").0, [UNICODE_INVALID, UNICODE_INVALID]);
    }

    #[test]
    fn decode_utf8_leaves_incomplete_input_pending() {
        let (output, data, pos) = decode(b"\xF0\x9F");
        assert!(output.is_empty());
        assert_eq!(data.bytes_remaining, 2);
        assert_eq!(data.bytes_total, 4);
        assert_eq!(pos, 2);
    }

    #[test]
    fn decode_usascii_decodes_the_gl_half() {
        let mut output = [0; 4];
        let mut cpi = 0;
        let mut pos = 0;
        decode_usascii(&mut output, &mut cpi, b"ABC", &mut pos);
        assert_eq!(&output[..cpi], &[b'A' as u32, b'B' as u32, b'C' as u32]);
        assert_eq!(pos, 3);
    }

    #[test]
    fn decode_usascii_decodes_the_gr_half_by_clearing_bit_eight() {
        let mut output = [0; 4];
        let mut cpi = 0;
        let mut pos = 0;
        decode_usascii(&mut output, &mut cpi, &[0xC1, 0xC2, 0xC3], &mut pos);
        assert_eq!(&output[..cpi], &[b'A' as u32, b'B' as u32, b'C' as u32]);
        assert_eq!(pos, 3);
    }

    #[test]
    fn decode_usascii_stops_before_controls_or_the_other_half() {
        let mut output = [0; 4];
        let mut cpi = 0;
        let mut pos = 0;
        decode_usascii(&mut output, &mut cpi, b"A\x1bB", &mut pos);
        assert_eq!(&output[..cpi], &[b'A' as u32]);
        assert_eq!(pos, 1);

        let mut cpi = 0;
        let mut pos = 0;
        decode_usascii(&mut output, &mut cpi, &[0xC1, b'B'], &mut pos);
        assert_eq!(&output[..cpi], &[b'A' as u32]);
        assert_eq!(pos, 1);
    }

    #[test]
    fn decode_usascii_honors_output_capacity() {
        let mut output = [0; 2];
        let mut cpi = 1;
        let mut pos = 0;
        decode_usascii(&mut output, &mut cpi, b"AB", &mut pos);
        assert_eq!(output[1], b'A' as u32);
        assert_eq!(cpi, 2);
        assert_eq!(pos, 1);
    }

    #[test]
    fn dec_special_graphics_table_matches_the_full_source_mapping() {
        assert_eq!(
            &DEC_SPECIAL_GRAPHICS[0x60..=0x7E],
            &[
                0x25C6, 0x2592, 0x2409, 0x240C, 0x240D, 0x240A, 0x00B0, 0x00B1,
                0x2424, 0x240B, 0x2518, 0x2510, 0x250C, 0x2514, 0x253C, 0x23BA,
                0x23BB, 0x2500, 0x23BC, 0x23BD, 0x251C, 0x2524, 0x2534, 0x252C,
                0x2502, 0x2A7D, 0x2A7E, 0x03C0, 0x2260, 0x00A3, 0x00B7,
            ]
        );
        assert!(DEC_SPECIAL_GRAPHICS[..0x60].iter().all(|&cp| cp == 0));
        assert_eq!(DEC_SPECIAL_GRAPHICS[0x7F], 0);
    }

    #[test]
    fn decode_table_maps_dec_graphics_and_preserves_unmapped_ascii() {
        let mut output = [0; 4];
        let mut cpi = 0;
        let mut pos = 0;
        decode_table(
            &DEC_SPECIAL_GRAPHICS,
            &mut output,
            &mut cpi,
            b"Aqx",
            &mut pos,
        );
        assert_eq!(&output[..cpi], &[b'A' as u32, 0x2500, 0x2502]);
        assert_eq!(pos, 3);
    }

    #[test]
    fn decode_table_handles_the_gr_half_and_stops_at_controls() {
        let mut output = [0; 4];
        let mut cpi = 0;
        let mut pos = 0;
        decode_table(
            &DEC_SPECIAL_GRAPHICS,
            &mut output,
            &mut cpi,
            &[0xF1, 0xF8, 0x9B],
            &mut pos,
        );
        assert_eq!(&output[..cpi], &[0x2500, 0x2502]);
        assert_eq!(pos, 2);
    }

    #[test]
    fn lookup_encoding_matches_the_complete_static_table() {
        assert_eq!(
            vterm_lookup_encoding(VTermEncodingType::Utf8, b'u'),
            Some(VTermEncoding::Utf8)
        );
        assert_eq!(
            vterm_lookup_encoding(VTermEncodingType::Single94, b'0'),
            Some(VTermEncoding::DecSpecialGraphics)
        );
        assert_eq!(
            vterm_lookup_encoding(VTermEncodingType::Single94, b'B'),
            Some(VTermEncoding::UsAscii)
        );
    }

    #[test]
    fn lookup_encoding_rejects_wrong_classes_and_designations() {
        assert_eq!(
            vterm_lookup_encoding(VTermEncodingType::Utf8, b'B'),
            None
        );
        assert_eq!(
            vterm_lookup_encoding(VTermEncodingType::Single94, b'u'),
            None
        );
        assert_eq!(
            vterm_lookup_encoding(VTermEncodingType::Single94, b'A'),
            None
        );
    }

    #[test]
    fn encoding_instance_selects_and_initializes_utf8() {
        let mut instance = VTermEncodingInstance::new(VTermEncoding::Utf8);
        assert_eq!(instance.encoding(), VTermEncoding::Utf8);
        instance.utf8_data.bytes_remaining = 3;
        instance.utf8_data.bytes_total = 4;
        instance.utf8_data.this_cp = 0x1234;
        instance.reset();
        assert_eq!(instance.utf8_data.bytes_remaining, 0);
        assert_eq!(instance.utf8_data.bytes_total, 0);
        assert_eq!(instance.utf8_data.this_cp, 0x1234);
    }

    #[test]
    fn encoding_instance_reset_is_a_noop_for_stateless_encodings() {
        for encoding in [
            VTermEncoding::UsAscii,
            VTermEncoding::DecSpecialGraphics,
        ] {
            let mut instance = VTermEncodingInstance::new(encoding);
            instance.utf8_data = Utf8DecoderData {
                bytes_remaining: 1,
                bytes_total: 2,
                this_cp: 3,
            };
            instance.reset();
            assert_eq!(instance.encoding(), encoding);
            assert_eq!(instance.utf8_data.bytes_remaining, 1);
            assert_eq!(instance.utf8_data.bytes_total, 2);
            assert_eq!(instance.utf8_data.this_cp, 3);
        }
    }

    #[test]
    fn encoding_instance_dispatches_every_static_decoder() {
            let cases: [(VTermEncoding, &[u8], &[u32]); 3] = [
                (VTermEncoding::Utf8, b"\xE2\x82\xAC", &[0x20AC]),
                (VTermEncoding::DecSpecialGraphics, b"qx", &[0x2500, 0x2502]),
                (VTermEncoding::UsAscii, b"AB", &[b'A' as u32, b'B' as u32]),
            ];
            for (encoding, input, expected) in cases {
                let mut instance = VTermEncodingInstance::new(encoding);
                let mut output = [0; 4];
                let mut cpi = 0;
                let mut pos = 0;
                instance.decode(&mut output, &mut cpi, input, &mut pos);
                assert_eq!(&output[..cpi], expected);
                assert_eq!(pos, input.len());
            }
        }

    #[test]
    fn encoding_instance_preserves_utf8_state_between_decode_calls() {
            let mut instance = VTermEncodingInstance::new(VTermEncoding::Utf8);
            let mut output = [0; 4];
            let mut cpi = 0;
            let mut pos = 0;
            instance.decode(&mut output, &mut cpi, b"\xF0\x9F", &mut pos);
            assert_eq!(cpi, 0);
            assert_eq!(instance.utf8_data.bytes_remaining, 2);

            pos = 0;
            instance.decode(&mut output, &mut cpi, b"\x98\x80", &mut pos);
            assert_eq!(&output[..cpi], &[0x1F600]);
            assert_eq!(instance.utf8_data.bytes_remaining, 0);
    }
}
