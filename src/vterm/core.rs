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
}
