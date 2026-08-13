//! Translated from `src/nvim/spell_defs.h` (initial core).

/// One spell replacement, from `ft_from` to `ft_to` (`fromto_T`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FromtoT {
    pub ft_from: Option<Vec<u8>>,
    pub ft_to: Option<Vec<u8>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fromto_default_has_no_replacement_strings() {
        assert_eq!(FromtoT::default(), FromtoT {
            ft_from: None,
            ft_to: None,
        });
    }
}
