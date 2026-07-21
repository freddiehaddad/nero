//! Translated from `src/nvim/sign_defs.h`.

use crate::decoration_defs::DecorSignHighlight;
use crate::types_defs::{ScharT, SIGN_WIDTH};

/// Sign attributes. Used by the screen refresh routines (`SignTextAttrs`).
#[derive(Debug, Clone, Copy)]
pub struct SignTextAttrs {
    pub text: [ScharT; SIGN_WIDTH as usize],
    pub hl_id: i32,
}

/// Struct to hold the sign properties (`sign_T`).
#[derive(Debug, Clone)]
pub struct SignT {
    /// name of sign
    pub sn_name: Vec<u8>,
    /// name of pixmap
    pub sn_icon: Option<Vec<u8>>,
    /// text used instead of pixmap
    pub sn_text: [ScharT; SIGN_WIDTH as usize],
    /// highlight ID for line
    pub sn_line_hl: i32,
    /// highlight ID for text
    pub sn_text_hl: i32,
    /// highlight ID for text on current line when `'cursorline'` is set
    pub sn_cul_hl: i32,
    /// highlight ID for line number
    pub sn_num_hl: i32,
    /// default priority of this sign, `-1` means [`SIGN_DEF_PRIO`]
    pub sn_priority: i32,
}

#[derive(Debug, Clone)]
pub struct SignItem {
    pub sh: Option<Box<DecorSignHighlight>>,
    pub id: u32,
}

/// Maximum number of signs shown on a single line (`SIGN_SHOW_MAX`).
pub const SIGN_SHOW_MAX: i32 = 9;
/// Default sign highlight priority (`SIGN_DEF_PRIO`).
pub const SIGN_DEF_PRIO: i32 = 10;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_text_attrs_has_sign_width_cells() {
        let attrs = SignTextAttrs { text: [0; SIGN_WIDTH as usize], hl_id: 0 };
        assert_eq!(attrs.text.len(), 2);
    }

    #[test]
    fn sign_def_prio_used_as_sn_priority_sentinel() {
        // -1 (not SIGN_DEF_PRIO) is the documented "use the default" sentinel.
        let sign = SignT {
            sn_name: b"test".to_vec(),
            sn_icon: None,
            sn_text: [0; SIGN_WIDTH as usize],
            sn_line_hl: 0,
            sn_text_hl: 0,
            sn_cul_hl: 0,
            sn_num_hl: 0,
            sn_priority: -1,
        };
        assert_eq!(sign.sn_priority, -1);
        assert_ne!(sign.sn_priority, SIGN_DEF_PRIO);
    }
}
