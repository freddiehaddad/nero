//! Translated from `src/nvim/regexp_defs.h` (partial: just `optmagic_T`,
//! needed by `globals.h`'s `magic_overruled`).
//!
//! The bulk of this header (the compiled-regexp-program representation,
//! `regmatch_T`/`regmmatch_T`/`reg_extmatch_T`, etc.) belongs with the
//! regexp engine as a unit (phase 7) - deferred, not started. See
//! `types_defs.rs`'s `RegprogT`/`RegmatchT`/`RegExtmatchT` opaque
//! placeholders for the pieces referenced elsewhere before then.

/// While executing a regexp and set to `MagicOn`/`MagicOff` this
/// overrules `p_magic`. Otherwise set to `NotSet` (`optmagic_T`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OptmagicT {
    /// `p_magic` not overruled
    #[default]
    NotSet,
    /// magic on inside regexp
    MagicOn,
    /// magic off inside regexp
    MagicOff,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optmagic_default_is_not_set() {
        assert_eq!(OptmagicT::default(), OptmagicT::NotSet);
    }
}
