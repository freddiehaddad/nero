//! Translated from `src/nvim/regexp_defs.h` (partial: `optmagic_T`,
//! needed by `globals.h`'s `magic_overruled`, plus `NSUBEXP` and the
//! multi-line match result [`RegmmatchT`]).
//!
//! The engine that builds and runs compiled regexps remains deferred.
//! Its `regengine_T` stays opaque, while `regprog_T`'s small common
//! header is now translated in [`crate::types_defs::RegprogT`].
//!
//! `regmatch_T` (the single-line form) is deliberately NOT translated
//! here. Its `startp`/`endp` are `char *` pointing INTO the line being
//! matched, so modelling it forces a pointer-versus-offset decision
//! that should be made against its real users, not guessed at now.
//! `regmmatch_T` has no such problem: its positions are `lpos_T`
//! values. See `types_defs.rs`'s `RegmatchT` opaque placeholder for
//! the single-line result still referenced elsewhere.

/// Maximum number of sub-expressions, including the whole match at
/// index 0 (`NSUBEXP`).
pub const NSUBEXP: usize = 10;

/// Opaque regular-expression engine dispatch table (`regengine_T`).
pub struct RegengineT {
    _private: (),
}

/// The result of a multi-line regexp match (`regmmatch_T`).
///
/// Sub-match `no` starts at `startpos[no]` and ends just before
/// `endpos[no]`. Line numbers are RELATIVE to the first line of the
/// match, so `startpos[0].lnum` is always zero. A sub-match that did
/// not participate is marked by a negative `lnum`, which is what
/// [`crate::search::first_submatch`] scans for.
///
/// `regprog` is kept as a raw pointer to
/// [`crate::types_defs::RegprogT`], exactly as the original does.
#[derive(Debug, Clone, Copy)]
pub struct RegmmatchT {
    /// the compiled regexp program (`regprog`).
    pub regprog: *mut crate::types_defs::RegprogT,
    /// start of each sub-match (`startpos`).
    pub startpos: [crate::pos_defs::LposT; NSUBEXP],
    /// end of each sub-match (`endpos`).
    pub endpos: [crate::pos_defs::LposT; NSUBEXP],
    /// ignore case (`rmm_ic`).
    pub rmm_ic: i32,
    /// when not zero: maximum column (`rmm_maxcol`).
    pub rmm_maxcol: crate::pos_defs::ColnrT,
}

impl Default for RegmmatchT {
    /// A zeroed match, matching the original's `{0}` initialisers.
    ///
    /// Written out rather than derived because a raw pointer has no
    /// `Default`.
    fn default() -> Self {
        RegmmatchT {
            regprog: std::ptr::null_mut(),
            startpos: [crate::pos_defs::LposT::default(); NSUBEXP],
            endpos: [crate::pos_defs::LposT::default(); NSUBEXP],
            rmm_ic: 0,
            rmm_maxcol: 0,
        }
    }
}

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

/// Effective regular-expression magic level (`magic_T`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum MagicT {
    /// `\V`, very nomagic.
    None = 1,
    /// `\M`, nomagic.
    Off = 2,
    /// `\m` or the `'magic'` option.
    On = 3,
    /// `\v`, very magic.
    All = 4,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magic_level_discriminants_match_regexp_defs_h() {
        assert_eq!(MagicT::None as i32, 1);
        assert_eq!(MagicT::Off as i32, 2);
        assert_eq!(MagicT::On as i32, 3);
        assert_eq!(MagicT::All as i32, 4);
        assert!(MagicT::All as i32 > MagicT::On as i32);
    }

    #[test]
    fn optmagic_default_is_not_set() {
        assert_eq!(OptmagicT::default(), OptmagicT::NotSet);
    }
}
