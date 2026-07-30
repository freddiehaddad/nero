//! Translated from `src/nvim/arabic.c` (partial).
//!
//! Translated: `arabic_maycombine`, `arabic_combine` - small,
//! self-contained predicates needed by `mbyte.c`'s
//! `utf_composinglike`/`utf_head_off`, translated alongside that
//! caller rather than waiting on the rest of `arabic.c` (Arabic
//! letter-shaping, a much larger and more specialized subsystem, out
//! of scope for this pass).
//!
//! Also translated: the `achars[]` presentation-form lookup table (as
//! [`ACHARS`]) + `find_achar` (as [`find_achar`]) + the `A_is_iso`/
//! `A_is_ok`/`A_is_valid`/`can_join` classification family built on
//! top of it - deliberately deferred in earlier sessions as "too
//! large given remaining context budget" (54 entries), but bounded
//! and mechanically verifiable: independently cross-checked via a
//! throwaway Python script (parsing the real `achars[]` array literal
//! directly, resolving each `a_*` enum name to its own codepoint, and
//! asserting the `c` column is strictly increasing - a precondition
//! `find_achar`'s own binary search silently depends on) before being
//! hand-transcribed here, with zero mismatches. `find_achar`/
//! `A_is_iso`/`A_is_ok`/`A_is_valid`/`can_join` have no REAL caller of
//! their own yet (their only real use, `arabic_shape`'s letter-shaping
//! state machine, remains out of scope - substantially larger and
//! more specialized, no current caller either), but are pure,
//! self-contained, and mechanically verifiable in isolation - the
//! same "translate ahead of the surrounding engine" precedent already
//! established repeatedly elsewhere in this crate (e.g. `sign.rs`'s
//! `sign_cmd_idx`, `menu.rs`'s `menu_is_hidden`).
//!
//! Deferred: `arabic_shape` itself (the actual letter-joining state
//! machine that consumes `can_join`/`A_is_valid`) - substantially
//! larger and more specialized, no current caller.

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

/// One entry of [`ACHARS`] (the original's own anonymous `static
/// struct achar { unsigned c; unsigned isolated; unsigned initial;
/// unsigned medial; unsigned final; }`). `final` is renamed `final_`
/// since `final` is a reserved word in Rust.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AChar {
    /// The plain Arabic codepoint itself (ISO-8859-6/Unicode) - the
    /// key [`find_achar`]'s own binary search looks up.
    pub c: u32,
    /// Isolated presentation form, `0` if none.
    pub isolated: u32,
    /// Initial presentation form, `0` if none.
    pub initial: u32,
    /// Medial presentation form, `0` if none.
    pub medial: u32,
    /// Final presentation form, `0` if none.
    pub final_: u32,
}

/// Sorted list of Unicode Arabic characters, each holding the
/// presentation forms of a letter (`achars`). Sorted by `c` (strictly
/// increasing), a precondition [`find_achar`]'s own binary search
/// depends on - independently verified via a throwaway Python script
/// before transcription (see this module's own doc comment).
///
/// Mechanically transcribed directly from the real `arabic.c` source
/// (not hand-typed from memory): every `a_*` enum name in the
/// original's own array literal was resolved to its real codepoint
/// value and cross-checked, index by index, before being written
/// here as a literal hex value.
pub const ACHARS: [AChar; 54] = [
    AChar { c: 0x0621, isolated: 0xfe80, initial: 0, medial: 0, final_: 0 }, // 0: a_HAMZA
    AChar { c: 0x0622, isolated: 0xfe81, initial: 0, medial: 0, final_: 0xfe82 }, // 1: a_ALEF_MADDA
    AChar { c: 0x0623, isolated: 0xfe83, initial: 0, medial: 0, final_: 0xfe84 }, // 2: a_ALEF_HAMZA_ABOVE
    AChar { c: 0x0624, isolated: 0xfe85, initial: 0, medial: 0, final_: 0xfe86 }, // 3: a_WAW_HAMZA
    AChar { c: 0x0625, isolated: 0xfe87, initial: 0, medial: 0, final_: 0xfe88 }, // 4: a_ALEF_HAMZA_BELOW
    AChar { c: 0x0626, isolated: 0xfe89, initial: 0xfe8b, medial: 0xfe8c, final_: 0xfe8a }, // 5: a_YEH_HAMZA
    AChar { c: 0x0627, isolated: 0xfe8d, initial: 0, medial: 0, final_: 0xfe8e }, // 6: a_ALEF
    AChar { c: 0x0628, isolated: 0xfe8f, initial: 0xfe91, medial: 0xfe92, final_: 0xfe90 }, // 7: a_BEH
    AChar { c: 0x0629, isolated: 0xfe93, initial: 0, medial: 0, final_: 0xfe94 }, // 8: a_TEH_MARBUTA
    AChar { c: 0x062a, isolated: 0xfe95, initial: 0xfe97, medial: 0xfe98, final_: 0xfe96 }, // 9: a_TEH
    AChar { c: 0x062b, isolated: 0xfe99, initial: 0xfe9b, medial: 0xfe9c, final_: 0xfe9a }, // 10: a_THEH
    AChar { c: 0x062c, isolated: 0xfe9d, initial: 0xfe9f, medial: 0xfea0, final_: 0xfe9e }, // 11: a_JEEM
    AChar { c: 0x062d, isolated: 0xfea1, initial: 0xfea3, medial: 0xfea4, final_: 0xfea2 }, // 12: a_HAH
    AChar { c: 0x062e, isolated: 0xfea5, initial: 0xfea7, medial: 0xfea8, final_: 0xfea6 }, // 13: a_KHAH
    AChar { c: 0x062f, isolated: 0xfea9, initial: 0, medial: 0, final_: 0xfeaa }, // 14: a_DAL
    AChar { c: 0x0630, isolated: 0xfeab, initial: 0, medial: 0, final_: 0xfeac }, // 15: a_THAL
    AChar { c: 0x0631, isolated: 0xfead, initial: 0, medial: 0, final_: 0xfeae }, // 16: a_REH
    AChar { c: 0x0632, isolated: 0xfeaf, initial: 0, medial: 0, final_: 0xfeb0 }, // 17: a_ZAIN
    AChar { c: 0x0633, isolated: 0xfeb1, initial: 0xfeb3, medial: 0xfeb4, final_: 0xfeb2 }, // 18: a_SEEN
    AChar { c: 0x0634, isolated: 0xfeb5, initial: 0xfeb7, medial: 0xfeb8, final_: 0xfeb6 }, // 19: a_SHEEN
    AChar { c: 0x0635, isolated: 0xfeb9, initial: 0xfebb, medial: 0xfebc, final_: 0xfeba }, // 20: a_SAD
    AChar { c: 0x0636, isolated: 0xfebd, initial: 0xfebf, medial: 0xfec0, final_: 0xfebe }, // 21: a_DAD
    AChar { c: 0x0637, isolated: 0xfec1, initial: 0xfec3, medial: 0xfec4, final_: 0xfec2 }, // 22: a_TAH
    AChar { c: 0x0638, isolated: 0xfec5, initial: 0xfec7, medial: 0xfec8, final_: 0xfec6 }, // 23: a_ZAH
    AChar { c: 0x0639, isolated: 0xfec9, initial: 0xfecb, medial: 0xfecc, final_: 0xfeca }, // 24: a_AIN
    AChar { c: 0x063a, isolated: 0xfecd, initial: 0xfecf, medial: 0xfed0, final_: 0xfece }, // 25: a_GHAIN
    AChar { c: 0x0640, isolated: 0, initial: 0x0640, medial: 0x0640, final_: 0x0640 }, // 26: a_TATWEEL
    AChar { c: 0x0641, isolated: 0xfed1, initial: 0xfed3, medial: 0xfed4, final_: 0xfed2 }, // 27: a_FEH
    AChar { c: 0x0642, isolated: 0xfed5, initial: 0xfed7, medial: 0xfed8, final_: 0xfed6 }, // 28: a_QAF
    AChar { c: 0x0643, isolated: 0xfed9, initial: 0xfedb, medial: 0xfedc, final_: 0xfeda }, // 29: a_KAF
    AChar { c: 0x0644, isolated: 0xfedd, initial: 0xfedf, medial: 0xfee0, final_: 0xfede }, // 30: a_LAM
    AChar { c: 0x0645, isolated: 0xfee1, initial: 0xfee3, medial: 0xfee4, final_: 0xfee2 }, // 31: a_MEEM
    AChar { c: 0x0646, isolated: 0xfee5, initial: 0xfee7, medial: 0xfee8, final_: 0xfee6 }, // 32: a_NOON
    AChar { c: 0x0647, isolated: 0xfee9, initial: 0xfeeb, medial: 0xfeec, final_: 0xfeea }, // 33: a_HEH
    AChar { c: 0x0648, isolated: 0xfeed, initial: 0, medial: 0, final_: 0xfeee }, // 34: a_WAW
    AChar { c: 0x0649, isolated: 0xfeef, initial: 0, medial: 0, final_: 0xfef0 }, // 35: a_ALEF_MAKSURA
    AChar { c: 0x064a, isolated: 0xfef1, initial: 0xfef3, medial: 0xfef4, final_: 0xfef2 }, // 36: a_YEH
    AChar { c: 0x064b, isolated: 0xfe70, initial: 0, medial: 0, final_: 0 }, // 37: a_FATHATAN
    AChar { c: 0x064c, isolated: 0xfe72, initial: 0, medial: 0, final_: 0 }, // 38: a_DAMMATAN
    AChar { c: 0x064d, isolated: 0xfe74, initial: 0, medial: 0, final_: 0 }, // 39: a_KASRATAN
    AChar { c: 0x064e, isolated: 0xfe76, initial: 0, medial: 0xfe77, final_: 0 }, // 40: a_FATHA
    AChar { c: 0x064f, isolated: 0xfe78, initial: 0, medial: 0xfe79, final_: 0 }, // 41: a_DAMMA
    AChar { c: 0x0650, isolated: 0xfe7a, initial: 0, medial: 0xfe7b, final_: 0 }, // 42: a_KASRA
    AChar { c: 0x0651, isolated: 0xfe7c, initial: 0, medial: 0xfe7c, final_: 0 }, // 43: a_SHADDA
    AChar { c: 0x0652, isolated: 0xfe7e, initial: 0, medial: 0xfe7f, final_: 0 }, // 44: a_SUKUN
    AChar { c: 0x0653, isolated: 0, initial: 0, medial: 0, final_: 0 }, // 45: a_MADDA_ABOVE
    AChar { c: 0x0654, isolated: 0, initial: 0, medial: 0, final_: 0 }, // 46: a_HAMZA_ABOVE
    AChar { c: 0x0655, isolated: 0, initial: 0, medial: 0, final_: 0 }, // 47: a_HAMZA_BELOW
    AChar { c: 0x067e, isolated: 0xfb56, initial: 0xfb58, medial: 0xfb59, final_: 0xfb57 }, // 48: a_PEH
    AChar { c: 0x0686, isolated: 0xfb7a, initial: 0xfb7c, medial: 0xfb7d, final_: 0xfb7b }, // 49: a_TCHEH
    AChar { c: 0x0698, isolated: 0xfb8a, initial: 0, medial: 0, final_: 0xfb8b }, // 50: a_JEH
    AChar { c: 0x06a9, isolated: 0xfb8e, initial: 0xfb90, medial: 0xfb91, final_: 0xfb8f }, // 51: a_FKAF
    AChar { c: 0x06af, isolated: 0xfb92, initial: 0xfb94, medial: 0xfb95, final_: 0xfb93 }, // 52: a_GAF
    AChar { c: 0x06cc, isolated: 0xfbfc, initial: 0xfbfe, medial: 0xfbff, final_: 0xfbfd }, // 53: a_FYEH
];

/// `a_HAMZA` - the one entry [`a_is_valid`] excludes despite being a
/// valid Arabic ISO-8859-6 character (`arabic.c`'s own enum value,
/// duplicated here since only this one entry needs it by name).
const A_HAMZA: i32 = 0x0621;

/// `a_BYTE_ORDER_MARK` - the single non-`achars`-table codepoint
/// [`a_is_ok`] also accepts.
const A_BYTE_ORDER_MARK: i32 = 0xfeff;

/// Find the [`AChar`] entry for the given Arabic character, `None` if
/// not found (`find_achar`). Binary search over [`ACHARS`], relying on
/// its own strictly-increasing `c` ordering (verified when the table
/// was transcribed - see this module's own doc comment).
#[must_use]
pub fn find_achar(c: i32) -> Option<&'static AChar> {
    let c = u32::try_from(c).ok()?;
    ACHARS.binary_search_by_key(&c, |a| a.c).ok().map(|i| &ACHARS[i])
}

/// `true` if `c` is an Arabic ISO-8859-6 character (alphabet/number/
/// punctuation) (`A_is_iso`).
#[must_use]
pub fn a_is_iso(c: i32) -> bool {
    find_achar(c).is_some()
}

/// `true` if `c` is an Arabic 10646 (8859-6 or Form-B) character
/// (`A_is_ok`).
#[must_use]
pub fn a_is_ok(c: i32) -> bool {
    a_is_iso(c) || c == A_BYTE_ORDER_MARK
}

/// `true` if `c` is an Arabic 10646 (8859-6 or Form-B) character, with
/// some exceptions/exclusions (`A_is_valid`).
#[must_use]
pub fn a_is_valid(c: i32) -> bool {
    a_is_ok(c) && c != A_HAMZA
}

/// Whether it is possible to join the given letters (`can_join`).
///
/// `c1` must have an initial or medial presentation form (i.e. it can
/// START a join), and `c2` must have a final or medial presentation
/// form (i.e. it can END one) - matches the original's own
/// `(a1->initial || a1->medial) && (a2->final || a2->medial)` exactly.
#[must_use]
pub fn can_join(c1: i32, c2: i32) -> bool {
    let (Some(a1), Some(a2)) = (find_achar(c1), find_achar(c2)) else {
        return false;
    };
    (a1.initial != 0 || a1.medial != 0) && (a2.final_ != 0 || a2.medial != 0)
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

    #[test]
    fn achars_table_has_exactly_54_entries_strictly_increasing_by_c() {
        assert_eq!(ACHARS.len(), 54);
        for pair in ACHARS.windows(2) {
            assert!(pair[0].c < pair[1].c, "{:#x} should be < {:#x}", pair[0].c, pair[1].c);
        }
        // Spot-check the first/last entries against the real source.
        assert_eq!(ACHARS[0].c, 0x0621); // a_HAMZA
        assert_eq!(ACHARS[0].isolated, 0xfe80);
        assert_eq!(ACHARS[53].c, 0x06cc); // a_FYEH
        assert_eq!(ACHARS[53].isolated, 0xfbfc);
    }

    #[test]
    fn find_achar_locates_first_middle_and_last_entries() {
        assert_eq!(find_achar(0x0621).unwrap().isolated, 0xfe80); // first: a_HAMZA
        assert_eq!(find_achar(0x0644).unwrap().isolated, 0xfedd); // middle: a_LAM
        assert_eq!(find_achar(0x06cc).unwrap().isolated, 0xfbfc); // last: a_FYEH
    }

    #[test]
    fn find_achar_returns_none_for_unknown_or_negative_codepoints() {
        assert!(find_achar(b'A' as i32).is_none());
        assert!(find_achar(0x0700).is_none()); // between blocks, not a real entry
        assert!(find_achar(-5).is_none());
    }

    #[test]
    fn a_is_iso_true_only_for_table_entries() {
        assert!(a_is_iso(0x0621)); // a_HAMZA
        assert!(!a_is_iso(b'A' as i32));
        assert!(!a_is_iso(0xfeff)); // BOM itself is not IN the table
    }

    #[test]
    fn a_is_ok_also_accepts_the_byte_order_mark() {
        assert!(a_is_ok(0x0621)); // a real table entry
        assert!(a_is_ok(0xfeff)); // BOM, accepted even though not in ACHARS
        assert!(!a_is_ok(b'A' as i32));
    }

    #[test]
    fn a_is_valid_excludes_hamza_but_allows_everything_else_ok() {
        assert!(!a_is_valid(0x0621)); // a_HAMZA specifically excluded
        assert!(a_is_valid(0x0627)); // a_ALEF: ok and not HAMZA
        // The original's own A_is_valid = A_is_ok(c) && c != a_HAMZA
        // has no special case for the BOM - it passes A_is_ok and
        // isn't a_HAMZA, so it IS considered "valid" here too,
        // faithfully matching the original rather than "fixing" what
        // might look like an odd allowance.
        assert!(a_is_valid(0xfeff));
        assert!(!a_is_valid(b'A' as i32));
    }

    #[test]
    fn can_join_true_when_first_can_start_and_second_can_end() {
        // a_BEH (initial=0xfe91, medial=0xfe92) can start a join;
        // a_TEH_MARBUTA (final=0xfe94) can end one.
        assert!(can_join(0x0628, 0x0629));
    }

    #[test]
    fn can_join_false_when_first_cannot_start_a_join() {
        // a_TEH_MARBUTA itself has neither an initial nor a medial
        // form, so it can never be the FIRST letter of a join.
        assert!(!can_join(0x0629, 0x0628));
    }

    #[test]
    fn can_join_false_when_second_cannot_end_a_join() {
        // a_HAMZA has neither a medial nor a final form, so it can
        // never be the SECOND letter of a join.
        assert!(!can_join(0x0628, 0x0621));
    }

    #[test]
    fn can_join_false_for_an_unknown_character_on_either_side() {
        assert!(!can_join(b'A' as i32, 0x0629));
        assert!(!can_join(0x0628, b'A' as i32));
    }
}
