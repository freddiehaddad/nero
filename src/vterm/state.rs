//! Translated from `src/nvim/vterm/state.c`.

/// Primary Device Attributes response (`vterm_primary_device_attr`).
pub static VTERM_PRIMARY_DEVICE_ATTR: &[u8] = b"61;22;52";

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn primary_device_attributes_match_state_c() {
        assert_eq!(VTERM_PRIMARY_DEVICE_ATTR, b"61;22;52");
    }
}
