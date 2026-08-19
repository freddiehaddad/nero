//! Translated from `src/nvim/map_glyph_cache.c`.
//!
//! Rust's generic [`crate::map::Set`] already implements the identical
//! open-addressed probing, tombstone, rehash, and put-status behavior.
//! Glyph keys remain owned byte vectors rather than a NUL-separated C
//! allocation; indices and observable cache semantics are unchanged.

pub type GlyphSet = crate::map::Set<Vec<u8>>;

/// Return the compact key index when `key` exists (`mh_get_glyph`).
#[must_use]
pub fn mh_get_glyph(set: &GlyphSet, key: &[u8]) -> Option<usize> {
    set.get_index(&key.to_vec())
}

/// Intern `key` and return its compact index and insertion status
/// (`mh_put_glyph`).
pub fn mh_put_glyph(
    set: &mut GlyphSet,
    key: &[u8],
) -> (usize, crate::map::MhPutStatus) {
    set.put(key.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyph_cache_interns_and_reuses_compact_indices() {
        let mut set = GlyphSet::new();
        let (first, status) = mh_put_glyph(&mut set, "é".as_bytes());
        assert_ne!(status, crate::map::MhPutStatus::Existing);
        assert_eq!(mh_get_glyph(&set, "é".as_bytes()), Some(first));
        assert_eq!(
            mh_put_glyph(&mut set, "é".as_bytes()),
            (first, crate::map::MhPutStatus::Existing)
        );
        assert_eq!(mh_get_glyph(&set, b"missing"), None);
    }
}
