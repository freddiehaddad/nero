//! Translated from `src/nvim/vterm/encoding.c`.
//!
//! This module starts with the state carried across calls to the UTF-8
//! decoder and its initialization routine. The decoder and the other
//! libvterm character sets are translated incrementally below.

const UNICODE_INVALID: u32 = 0xFFFD;

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
}
