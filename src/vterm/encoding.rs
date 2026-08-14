//! Translated from `src/nvim/vterm/encoding.c`.
//!
//! This module starts with the state carried across calls to the UTF-8
//! decoder and its initialization routine. The decoder and the other
//! libvterm character sets are translated incrementally below.

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
}
