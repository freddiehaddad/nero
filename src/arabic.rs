//! Translated from `src/nvim/arabic.c` (partial).
//!
//! Translated: `arabic_maycombine`, `arabic_combine` - small,
//! self-contained predicates needed by `mbyte.c`'s
//! `utf_composinglike`/`utf_head_off`, translated alongside that
//! caller rather than waiting on the rest of `arabic.c` (Arabic
//! letter-shaping, a much larger and more specialized subsystem, out
//! of scope for this pass).
//!
//! Deferred: everything else in `arabic.c` (letter shaping/joining
//! tables, `A_is_*` classification, `arabic_shape`) - no current
//! caller.

use crate::option_vars::OPTION_VARS;

/// Arabic ALEF-family codepoints relevant to [`arabic_maycombine`]
/// (`a_ALEF_MADDA`/`a_ALEF_HAMZA_ABOVE`/`a_ALEF_HAMZA_BELOW`/`a_ALEF`).
mod codepoint {
    pub const ALEF_MADDA: i32 = 0x0622;
    pub const ALEF_HAMZA_ABOVE: i32 = 0x0623;
    pub const ALEF_HAMZA_BELOW: i32 = 0x0625;
    pub const ALEF: i32 = 0x0627;
    /// `a_LAM`, relevant to [`super::arabic_combine`].
    pub const LAM: i32 = 0x0644;
}

/// Check whether we are dealing with a character that could be
/// regarded as an Arabic combining character, need to check the
/// character before this (`arabic_maycombine`).
///
/// # Safety
/// Touches `crate::option_vars::OPTION_VARS` - same requirement as
/// every other function that does so: no overlapping live access.
#[must_use]
pub unsafe fn arabic_maycombine(two: i32) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let opts = unsafe { OPTION_VARS.get_mut() };
    if opts.p_arshape != 0 && opts.p_tbidi == 0 {
        return two == codepoint::ALEF_MADDA
            || two == codepoint::ALEF_HAMZA_ABOVE
            || two == codepoint::ALEF_HAMZA_BELOW
            || two == codepoint::ALEF;
    }
    false
}

/// Check whether we are dealing with Arabic combining characters.
/// Returns `false` for negative values.
///
/// Note: these are NOT really composing characters!
///
/// @param one First character.
/// @param two Character just after `one` (`arabic_combine`).
///
/// # Safety
/// Same as [`arabic_maycombine`].
#[must_use]
pub unsafe fn arabic_combine(one: i32, two: i32) -> bool {
    if one == codepoint::LAM {
        // SAFETY: forwarded from this function's own safety doc.
        return unsafe { arabic_maycombine(two) };
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serializes tests that mutate `OPTION_VARS` (shared global
    /// state). Delegates to the crate-wide
    /// `crate::globals::global_state_test_lock` (shared by every file
    /// touching `GLOBALS`/`OPTION_VARS` in tests) - see that
    /// function's own doc comment for why a single shared lock is used.
    fn option_vars_test_lock() -> std::sync::MutexGuard<'static, ()> {
        crate::globals::global_state_test_lock()
    }

    #[test]
    fn arabic_combine_is_false_when_arshape_disabled() {
        let _lock = option_vars_test_lock();
        let opts = unsafe { OPTION_VARS.get_mut() };
        let (prev_arshape, prev_tbidi) = (opts.p_arshape, opts.p_tbidi);
        opts.p_arshape = 0;
        opts.p_tbidi = 0;

        assert!(!unsafe { arabic_combine(0x0644, 0x0627) }); // LAM, ALEF

        let opts = unsafe { OPTION_VARS.get_mut() };
        opts.p_arshape = prev_arshape;
        opts.p_tbidi = prev_tbidi;
    }

    #[test]
    fn arabic_combine_is_true_for_lam_alef_when_arshape_enabled() {
        let _lock = option_vars_test_lock();
        let opts = unsafe { OPTION_VARS.get_mut() };
        let (prev_arshape, prev_tbidi) = (opts.p_arshape, opts.p_tbidi);
        opts.p_arshape = 1;
        opts.p_tbidi = 0;

        assert!(unsafe { arabic_combine(0x0644, 0x0627) }); // LAM, ALEF
        assert!(!unsafe { arabic_combine(0x0644, b'A' as i32) }); // LAM, non-alef
        assert!(!unsafe { arabic_combine(b'A' as i32, 0x0627) }); // non-LAM, ALEF

        let opts = unsafe { OPTION_VARS.get_mut() };
        opts.p_arshape = prev_arshape;
        opts.p_tbidi = prev_tbidi;
    }

    #[test]
    fn arabic_combine_is_false_when_tbidi_enabled() {
        // 'termbidi' being on means the terminal itself handles Arabic
        // shaping, so nvim's own arabic_maycombine deliberately
        // disengages (matches the original's `p_arshape && !p_tbidi`
        // condition).
        let _lock = option_vars_test_lock();
        let opts = unsafe { OPTION_VARS.get_mut() };
        let (prev_arshape, prev_tbidi) = (opts.p_arshape, opts.p_tbidi);
        opts.p_arshape = 1;
        opts.p_tbidi = 1;

        assert!(!unsafe { arabic_combine(0x0644, 0x0627) });

        let opts = unsafe { OPTION_VARS.get_mut() };
        opts.p_arshape = prev_arshape;
        opts.p_tbidi = prev_tbidi;
    }
}
