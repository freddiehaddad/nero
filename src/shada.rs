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
}
