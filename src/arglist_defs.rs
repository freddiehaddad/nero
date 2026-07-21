//! Translated from `src/nvim/arglist_defs.h`.

use crate::garray_defs::GarrayT;

/// Argument list: Array of file names (`alist_T`).
/// Used for the global argument list and the argument lists local to a
/// window.
#[derive(Default)]
pub struct AlistT {
    /// growarray with the array of file names
    pub al_ga: GarrayT,
    /// number of windows using this arglist
    pub al_refcount: i32,
    /// id of this arglist
    pub id: i32,
}

/// For each argument remember the file name as it was given, and the
/// buffer number that contains the expanded file name (required for when
/// `":cd"` is used) (`aentry_T`).
#[derive(Debug, Clone)]
pub struct AentryT {
    /// file name as specified
    pub ae_fname: Vec<u8>,
    /// buffer number with expanded file name
    pub ae_fnum: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alist_default_starts_empty() {
        let a = AlistT::default();
        assert_eq!(a.al_refcount, 0);
        assert_eq!(a.id, 0);
        assert!(a.al_ga.is_empty());
    }
}
