//! Translated from `src/nvim/base64.c`/`base64.h`.
//!
//! The original reads 8 (or 4) input bytes at a time into a `uint64_t`
//! (`uint32_t`), byte-swaps to big-endian, then extracts 6-bit groups via
//! shifts - a word-at-a-time optimization of standard base64 encoding, and
//! the reason for the `htobe64`/`vim_htobe64` endian-conversion helpers.
//! This translation produces the identical output using the plain
//! byte-at-a-time algorithm directly (`chunks_exact(3)`), which needs no
//! endian conversion at all since it never reinterprets bytes as a machine
//! word - so `vim_htobe64`/`vim_htobe32` have no Rust counterpart.

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// `char_to_index` (1-based; `None` means "not part of the alphabet",
/// matching the original's `0` sentinel).
#[inline]
fn char_to_index(c: u8) -> Option<u8> {
    match c {
        b'A'..=b'Z' => Some(c - b'A' + 1),
        b'a'..=b'z' => Some(c - b'a' + 27),
        b'0'..=b'9' => Some(c - b'0' + 53),
        b'+' => Some(63),
        b'/' => Some(64),
        _ => None,
    }
}

/// Encode a byte string using Base64 (`base64_encode`).
pub fn base64_encode(src: &[u8]) -> std::string::String {
    let out_len = src.len().div_ceil(3) * 4;
    let mut dest = vec![0u8; out_len];
    let mut out_i = 0;

    let mut chunks = src.chunks_exact(3);
    for chunk in &mut chunks {
        dest[out_i] = ALPHABET[(chunk[0] >> 2) as usize];
        dest[out_i + 1] = ALPHABET[(((chunk[0] & 0x3) << 4) | (chunk[1] >> 4)) as usize];
        dest[out_i + 2] = ALPHABET[(((chunk[1] & 0xF) << 2) | (chunk[2] >> 6)) as usize];
        dest[out_i + 3] = ALPHABET[(chunk[2] & 0x3F) as usize];
        out_i += 4;
    }

    match chunks.remainder() {
        [b0, b1] => {
            dest[out_i] = ALPHABET[(b0 >> 2) as usize];
            dest[out_i + 1] = ALPHABET[(((b0 & 0x3) << 4) | (b1 >> 4)) as usize];
            dest[out_i + 2] = ALPHABET[((b1 & 0xF) << 2) as usize];
            dest[out_i + 3] = b'=';
        }
        [b0] => {
            dest[out_i] = ALPHABET[(b0 >> 2) as usize];
            dest[out_i + 1] = ALPHABET[((b0 & 0x3) << 4) as usize];
            dest[out_i + 2] = b'=';
            dest[out_i + 3] = b'=';
        }
        [] => {}
        _ => unreachable!("chunks_exact(3)'s remainder is always < 3 bytes"),
    }

    // ALPHABET and '=' are all ASCII, so this is always valid UTF-8.
    std::string::String::from_utf8(dest).unwrap()
}

/// Decode a Base64 encoded byte string (`base64_decode`).
///
/// Returns `None` on any malformed input (matches the original's `NULL` +
/// `*out_lenp = 0` on the `invalid:` path). Unlike the original, the
/// decoded bytes are returned as an owned `Vec<u8>` (may contain embedded
/// NULs, exactly like the original's non-NUL-terminated result) instead of
/// a raw pointer + separate out-param length.
pub fn base64_decode(src: &[u8]) -> Option<Vec<u8>> {
    if !src.len().is_multiple_of(4) {
        return None;
    }

    let mut out_len = (src.len() / 4) * 3;
    if !src.is_empty() && src[src.len() - 1] == b'=' {
        out_len -= 1;
    }
    if src.len() >= 2 && src[src.len() - 2] == b'=' {
        out_len -= 1;
    }

    let mut dest = Vec::with_capacity(out_len);
    let mut acc: i32 = 0;
    let mut acc_len: i32 = 0;
    let mut leftover_i: Option<usize> = None;

    let mut src_i = 0;
    while src_i < src.len() {
        let c = src[src_i];
        match char_to_index(c) {
            None => {
                if c == b'=' {
                    leftover_i = Some(src_i);
                    break;
                }
                return None;
            }
            Some(d) => {
                acc = ((acc << 6) & 0xFFF) + (d as i32 - 1);
                acc_len += 6;
                if acc_len >= 8 {
                    acc_len -= 8;
                    dest.push((acc >> acc_len) as u8);
                }
            }
        }
        src_i += 1;
    }

    if acc_len > 4 || (acc & ((1 << acc_len) - 1)) != 0 {
        return None;
    }

    if let Some(mut leftover_i) = leftover_i {
        let padding_len = acc_len / 2;
        let mut padding_chars = 0;
        while leftover_i < src.len() {
            if src[leftover_i] != b'=' {
                return None;
            }
            padding_chars += 1;
            leftover_i += 1;
        }
        if padding_chars != padding_len {
            return None;
        }
    }

    Some(dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_well_known_test_vector() {
        assert_eq!(base64_encode(b"Hello, World!"), "SGVsbG8sIFdvcmxkIQ==");
        assert_eq!(
            base64_decode(b"SGVsbG8sIFdvcmxkIQ==").unwrap(),
            b"Hello, World!"
        );
    }

    #[test]
    fn round_trips_for_every_remainder_length() {
        for len in 0..12 {
            let data: Vec<u8> = (0..len).map(|i| (i * 7 + 3) as u8).collect();
            let encoded = base64_encode(&data);
            let decoded = base64_decode(encoded.as_bytes()).unwrap();
            assert_eq!(decoded, data, "round trip failed for len={len}");
        }
    }

    #[test]
    fn empty_input_encodes_to_empty_string() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_decode(b"").unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn decode_rejects_bad_length() {
        assert_eq!(base64_decode(b"abc"), None); // not a multiple of 4
    }

    #[test]
    fn decode_rejects_invalid_characters() {
        assert_eq!(base64_decode(b"ab!d"), None);
    }

    #[test]
    fn decode_rejects_padding_in_the_wrong_place() {
        assert_eq!(base64_decode(b"=bcd"), None);
        assert_eq!(base64_decode(b"ab=d"), None);
    }
}
