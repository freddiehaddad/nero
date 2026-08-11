//! Translated from `src/nvim/shada.c`.
//!
//! ShaDa serialisation/deserialisation is still largely coupled to
//! MessagePack, history, registers and marks. This module starts with
//! the small representation-independent helpers and grows alongside
//! those dependencies.

/// Number of extra MessagePack items stored after a ShaDa entry's
/// fixed fields (`additional_data_len`).
///
/// A missing `AdditionalData` pointer contributes no items.
#[must_use]
pub fn additional_data_len(src: Option<&crate::types_defs::AdditionalData>) -> u32 {
    src.map_or(0, |data| data.nitems)
}

/// Finds parameter `kind` in a `'shada'` option value
/// (`find_shada_parameter`).
///
/// The returned slice starts immediately after the parameter letter,
/// just like the original's returned pointer. Only the first byte of
/// each comma-separated option part is examined. `n` is always the
/// final parameter, so a nonmatching `n` stops the scan.
#[must_use]
pub fn find_shada_parameter(shada: &[u8], kind: u8) -> Option<&[u8]> {
    let mut start = 0;
    while start < shada.len() {
        let current = shada[start];
        if current == kind {
            return Some(&shada[start + 1..]);
        }
        if current == b'n' {
            break;
        }
        let Some(comma) = shada[start..].iter().position(|&b| b == b',') else {
            break;
        };
        start += comma + 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn additional_data_len_is_zero_without_extra_data() {
        assert_eq!(additional_data_len(None), 0);
    }

    #[test]
    fn additional_data_len_reports_nitems_not_nbytes() {
        let data = crate::types_defs::AdditionalData {
            nitems: 3,
            nbytes: 99,
        };

        assert_eq!(additional_data_len(Some(&data)), 3);
    }

    #[test]
    fn additional_data_len_preserves_a_zero_item_header() {
        let data = crate::types_defs::AdditionalData {
            nitems: 0,
            nbytes: 12,
        };

        assert_eq!(additional_data_len(Some(&data)), 0);
    }

    #[test]
    fn find_shada_parameter_returns_the_suffix_after_the_kind() {
        assert_eq!(
            find_shada_parameter(b"'100,<50,s10", b'<'),
            Some(&b"50,s10"[..])
        );
        assert_eq!(
            find_shada_parameter(b"'100,<50,s10", b'\''),
            Some(&b"100,<50,s10"[..])
        );
    }

    #[test]
    fn find_shada_parameter_checks_only_option_part_starts() {
        assert_eq!(find_shada_parameter(b"'100,<50", b'1'), None);
        assert_eq!(find_shada_parameter(b"'100,<50", b'5'), None);
    }

    #[test]
    fn find_shada_parameter_stops_at_the_n_parameter() {
        // `n` is always last in a valid option. A malformed suffix
        // after it must not be searched.
        assert_eq!(find_shada_parameter(b"'100,n/tmp/file,<50", b'<'), None);
        assert_eq!(
            find_shada_parameter(b"'100,n/tmp/file", b'n'),
            Some(&b"/tmp/file"[..])
        );
    }

    #[test]
    fn find_shada_parameter_returns_none_for_missing_or_empty_values() {
        assert_eq!(find_shada_parameter(b"'100,<50", b's'), None);
        assert_eq!(find_shada_parameter(b"", b'\''), None);
    }
}
