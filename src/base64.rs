//! Translated from `src/nvim/base64.c`/`base64.h`.
//!
//! The encoder preserves the original's word-at-a-time fast paths: read
//! 8 input bytes to encode 6, then 4 input bytes to encode 3, convert each
//! word to big endian, and extract 6-bit groups by shifting.

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

const fn build_char_to_index() -> [u8; 256] {
    let mut table = [0; 256];
    let mut i = 0;
    while i < 26 {
        table[b'A' as usize + i] = i as u8 + 1;
        table[b'a' as usize + i] = i as u8 + 27;
        i += 1;
    }
    i = 0;
    while i < 10 {
        table[b'0' as usize + i] = i as u8 + 53;
        i += 1;
    }
    table[b'+' as usize] = 63;
    table[b'/' as usize] = 64;
    table
}

/// One-based alphabet indices (`char_to_index`).
///
/// Zero means that the byte is not part of the Base64 alphabet.
const CHAR_TO_INDEX: [u8; 256] = build_char_to_index();

/// Convert a host-order 64-bit word to big endian (`vim_htobe64`).
#[must_use]
pub const fn vim_htobe64(value: u64) -> u64 {
    value.to_be()
}

/// Convert a host-order 32-bit word to big endian (`vim_htobe32`).
#[must_use]
pub const fn vim_htobe32(value: u32) -> u32 {
    value.to_be()
}

/// Encode a byte string using Base64 (`base64_encode`).
pub fn base64_encode(src: &[u8]) -> std::string::String {
    let out_len = src.len().div_ceil(3) * 4;
    let mut dest = vec![0u8; out_len];
    let mut src_i = 0;
    let mut out_i = 0;

    while src_i + 7 < src.len() {
        let bits_h = u64::from_ne_bytes(src[src_i..src_i + 8].try_into().unwrap());
        let bits_be = vim_htobe64(bits_h);
        dest[out_i] = ALPHABET[((bits_be >> 58) & 0x3f) as usize];
        dest[out_i + 1] = ALPHABET[((bits_be >> 52) & 0x3f) as usize];
        dest[out_i + 2] = ALPHABET[((bits_be >> 46) & 0x3f) as usize];
        dest[out_i + 3] = ALPHABET[((bits_be >> 40) & 0x3f) as usize];
        dest[out_i + 4] = ALPHABET[((bits_be >> 34) & 0x3f) as usize];
        dest[out_i + 5] = ALPHABET[((bits_be >> 28) & 0x3f) as usize];
        dest[out_i + 6] = ALPHABET[((bits_be >> 22) & 0x3f) as usize];
        dest[out_i + 7] = ALPHABET[((bits_be >> 16) & 0x3f) as usize];
        src_i += 6;
        out_i += 8;
    }

    while src_i + 3 < src.len() {
        let bits_h = u32::from_ne_bytes(src[src_i..src_i + 4].try_into().unwrap());
        let bits_be = vim_htobe32(bits_h);
        dest[out_i] = ALPHABET[((bits_be >> 26) & 0x3f) as usize];
        dest[out_i + 1] = ALPHABET[((bits_be >> 20) & 0x3f) as usize];
        dest[out_i + 2] = ALPHABET[((bits_be >> 14) & 0x3f) as usize];
        dest[out_i + 3] = ALPHABET[((bits_be >> 8) & 0x3f) as usize];
        src_i += 3;
        out_i += 4;
    }

    match &src[src_i..] {
        [b0, b1, b2] => {
            dest[out_i] = ALPHABET[(b0 >> 2) as usize];
            dest[out_i + 1] = ALPHABET[(((b0 & 0x3) << 4) | (b1 >> 4)) as usize];
            dest[out_i + 2] = ALPHABET[(((b1 & 0xf) << 2) | (b2 >> 6)) as usize];
            dest[out_i + 3] = ALPHABET[(b2 & 0x3f) as usize];
        }
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
        _ => unreachable!("the word fast paths leave at most 3 bytes"),
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
        let d = CHAR_TO_INDEX[c as usize];
        if d == 0 {
            if c == b'=' {
                leftover_i = Some(src_i);
                break;
            }
            return None;
        }
        acc = ((acc << 6) & 0xFFF) + (d as i32 - 1);
        acc_len += 6;
        if acc_len >= 8 {
            acc_len -= 8;
            dest.push((acc >> acc_len) as u8);
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
        for len in 0..32 {
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

    #[test]
    fn endian_helpers_match_big_endian_byte_order() {
        assert_eq!(
            vim_htobe64(0x0102_0304_0506_0708).to_ne_bytes(),
            0x0102_0304_0506_0708_u64.to_be_bytes()
        );
        assert_eq!(
            vim_htobe32(0x0102_0304).to_ne_bytes(),
            0x0102_0304_u32.to_be_bytes()
        );
    }

    #[test]
    fn word_fast_paths_match_known_output() {
        assert_eq!(
            base64_encode(b"0123456789abcdef"),
            "MDEyMzQ1Njc4OWFiY2RlZg=="
        );
    }

    #[test]
    fn decode_table_matches_the_alphabet_and_zero_sentinel() {
        for (idx, byte) in ALPHABET.iter().copied().enumerate() {
            assert_eq!(CHAR_TO_INDEX[byte as usize], idx as u8 + 1);
        }
        for byte in 0u8..=u8::MAX {
            if !ALPHABET.contains(&byte) {
                assert_eq!(CHAR_TO_INDEX[byte as usize], 0);
            }
        }
    }
}
