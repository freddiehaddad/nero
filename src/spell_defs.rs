//! Translated from `src/nvim/spell_defs.h` (initial core).

/// One spell replacement, from `ft_from` to `ft_to` (`fromto_T`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FromtoT {
    pub ft_from: Option<Vec<u8>>,
    pub ft_to: Option<Vec<u8>>,
}

/// One sound-alike replacement rule (`salitem_T`).
///
/// `sm_oneof` and `sm_rules` are borrowed pointers into `sm_lead` in C;
/// byte offsets preserve that relationship without a self-referential
/// Rust struct.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SalitemT {
    pub sm_lead: Option<Vec<u8>>,
    pub sm_leadlen: i32,
    pub sm_oneof: Option<usize>,
    pub sm_rules: Option<usize>,
    pub sm_to: Option<Vec<u8>>,
    pub sm_lead_w: Option<Vec<i32>>,
    pub sm_oneof_w: Option<Vec<i32>>,
    pub sm_to_w: Option<Vec<i32>>,
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

    #[test]
    fn salitem_offsets_can_refer_into_the_owned_lead() {
        let item = SalitemT {
            sm_lead: Some(b"ab(cd)^".to_vec()),
            sm_leadlen: 2,
            sm_oneof: Some(2),
            sm_rules: Some(6),
            ..Default::default()
        };
        let lead = item.sm_lead.as_deref().expect("lead");
        assert_eq!(&lead[item.sm_oneof.expect("oneof")..], b"(cd)^");
        assert_eq!(&lead[item.sm_rules.expect("rules")..], b"^");
    }
}
