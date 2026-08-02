//! Translated from `src/nvim/spell.c` (tractable core only).
//!
//! `spell.c` (~3700 lines) implements neovim's spell-checking engine -
//! almost entirely dependent on the spell-file/word-tree/suggestion
//! machinery (`slang_T`, `matchinf_T`, etc.), none of which is
//! translated.
//!
//! Translated: [`byte_in_str`] (a pure `strchr`-equivalent),
//! [`spell_valid_case`] (a pure bitflag check over a small subset of
//! the original's `WF_*` word-flag bits - only the 4 needed here are
//! transcribed, not the whole enum), [`find_region`] (a pure lookup
//! over a 2-byte-per-entry region-code string), and
//! [`valid_spelllang`]/[`valid_spellfile`] (re-investigated after
//! being stale-grouped below with functions genuinely needing
//! `slang_T`'s own spell-file-loaded state - these two option-value
//! validators actually need only already-existing
//! `crate::option::{valid_name, copy_option_part}`/
//! `crate::charset::vim_is_fname_char`). [`spell_check_window`]/
//! [`no_spell_checking`] (whether spell checking is enabled/usable
//! for a window - needed only already-real `WinT.w_onebuf_opt.
//! wo_spell`/`w_s`'s own `b_p_spl`/`b_langp` fields; the latter's own
//! real `emsg` display is skipped, matching this crate's established
//! policy).
//!
//! Also translated: [`SpelltabT`]/[`clear_spell_chartab`]/
//! [`init_spell_chartab`] (from `spell_defs.h`/`spell.c` - no
//! dedicated `spell_defs.rs` module exists yet, matching
//! `charset.h`'s own "embedded directly, documented" precedent for a
//! header with no other translated members). Needed only already-real
//! `mbyte.c`'s `utf_fold`/`mb_toupper`/`mb_isupper`/`mb_islower`.
//!
//! Also translated: [`spell_is_upper`] (the `SPELL_ISUPPER` macro) and
//! [`spell_iswordp_nmw`] (the ASCII/Latin-1 `c <= 255` fast path via
//! `SPELLTAB`; panics for `c > 255`, which needs `mb_get_class`/
//! `utf_class`'s own real `curbuf->b_chartab`, still not translated -
//! the SAME `g_chartab` blocker documented throughout this crate's
//! `charset.rs`) and [`captype`] (the full two-phase
//! allcap/firstcap/past_second state machine, needing only
//! `spell_iswordp_nmw` and already-real `mbyte.c` primitives) - all
//! translated ahead of their own real caller (`find_word`/
//! `spell_check`, needing `slang_T`, not translated), matching this
//! crate's established "small, self-contained, no design freedom to
//! get wrong" precedent.
//!
//! Also translated: [`spell_enc`] (the effective spell-checking
//! encoding, `'encoding'` mapped to `"latin1"` for `"iso-8859-15"`) -
//! needed only already-real `option_vars.rs`'s `p_enc` field.
//! Translated ahead of its own real callers (`spellfile.c`'s spell-file
//! naming, needing `vim_snprintf`/file-templating, not translated).
//!
//! Also translated: [`nofold_len`] (case-folding may change byte
//! length; given `N` characters in a case-folded word's own byte-length
//! prefix, find the equivalent byte length of the same `N` characters
//! in the original, un-folded word) - a genuinely safe `fn` (unlike
//! this file's other `unsafe fn`s), since `crate::mbyte::utfc_ptr2len`
//! never reads out of bounds for any byte content.
//!
//! Also translated: [`spell_to_fold`]/[`spell_to_upper`] (the
//! `SPELL_TOFOLD`/`SPELL_TOUPPER` macros, siblings of
//! [`spell_is_upper`]) and [`onecap_copy`]/[`allcap_copy`]/
//! [`make_case_word`] (case-fixing a word copy - upper/fold the first
//! letter only, upper-case every letter including the real `ß` -> `"SS"`
//! quirk, or dispatch between the two by `WF_*` flag). Deviate from the
//! original's `wcopy[MAXWLEN]` fixed-size, truncating output buffer by
//! returning an unbounded, growing `Vec<u8>` instead - matching this
//! crate's established "growing `Vec` supersedes the manual
//! bounded-buffer C idiom" precedent (`winrestcmd`/
//! `vim_strsave_shellescape`); see [`onecap_copy`]'s own doc comment
//! for why this doesn't change observable behavior for any realistic
//! (non-pathologically-long) word.
//!
//! Deferred: everything else - `get_char_type`/`match_checkcompoundpattern`/
//! `can_compound`/`match_compoundrule`/`valid_word_prefix`/
//! `spell_casefold`/`check_need_cap`/`expand_spelling`, all needing
//! `slang_T`'s own spell-file-loaded state or the buffer/window
//! spell-option plumbing.

/// word has one capital (or all capitals) (`WF_ONECAP`).
pub const WF_ONECAP: i32 = 0x02;
/// word must be all capitals (`WF_ALLCAP`).
pub const WF_ALLCAP: i32 = 0x04;
/// keep-case word, all-cap not allowed (`WF_FIXCAP`).
pub const WF_FIXCAP: i32 = 0x40;
/// keep-case word (`WF_KEEPCAP`).
pub const WF_KEEPCAP: i32 = 0x80;

/// word valid in all regions (`REGION_ALL`).
pub const REGION_ALL: i32 = 0xff;

/// The tables used for recognizing word characters according to
/// spelling. These are only used for the first 256 characters of
/// `'encoding'` (`spelltab_T`, `spell_defs.h`).
pub struct SpelltabT {
    /// flags: is word char (`st_isw`)
    pub st_isw: [bool; 256],
    /// flags: is uppercase char (`st_isu`)
    pub st_isu: [bool; 256],
    /// chars: folded case (`st_fold`)
    pub st_fold: [u8; 256],
    /// chars: upper case (`st_upper`)
    pub st_upper: [u8; 256],
}

impl SpelltabT {
    /// A brand-new, all-zeroed table (matching the original's own
    /// static/zero-initialized `spelltab_T spelltab;` - every field
    /// starts `false`/`0` until [`clear_spell_chartab`]/
    /// [`init_spell_chartab`] populates it for real).
    #[must_use]
    pub const fn new() -> Self {
        SpelltabT { st_isw: [false; 256], st_isu: [false; 256], st_fold: [0; 256], st_upper: [0; 256] }
    }
}

impl Default for SpelltabT {
    fn default() -> Self {
        Self::new()
    }
}

/// The crate-wide `spelltab_T spelltab` global instance, matching
/// this crate's established `GlobalCell`-backed file-static
/// convention. Const-constructible (every field is a plain array),
/// so no `LazyLock` wrapper is needed.
static SPELLTAB: crate::globals::GlobalCell<SpelltabT> =
    crate::globals::GlobalCell::new(SpelltabT::new());

/// Whether a spell file has customized the chartab
/// (`did_set_spelltab`, `spell.h`'s own real cross-file `extern bool`,
/// also read/written by `spellfile.c`, not translated, so this is
/// currently only ever written by [`init_spell_chartab`] itself).
static DID_SET_SPELLTAB: crate::globals::GlobalCell<bool> = crate::globals::GlobalCell::new(false);

/// Resets `sp` to its baseline ASCII-only word-character table
/// (`clear_spell_chartab`): digits/letters are word characters,
/// `A`-`Z`/`a`-`z` fold to their opposite case, everything else
/// starts as "not a word character" with an identity fold/upper
/// mapping.
pub fn clear_spell_chartab(sp: &mut SpelltabT) {
    sp.st_isw = [false; 256];
    sp.st_isu = [false; 256];

    for i in 0..256usize {
        sp.st_fold[i] = i as u8;
        sp.st_upper[i] = i as u8;
    }

    for i in b'0'..=b'9' {
        sp.st_isw[i as usize] = true;
    }
    for i in b'A'..=b'Z' {
        sp.st_isw[i as usize] = true;
        sp.st_isu[i as usize] = true;
        sp.st_fold[i as usize] = i + 0x20;
    }
    for i in b'a'..=b'z' {
        sp.st_isw[i as usize] = true;
        sp.st_upper[i as usize] = i - 0x20;
    }
}

/// Initializes the chartab used for spelling (`init_spell_chartab`).
/// Called once while starting up. The default is ASCII-only (via
/// [`clear_spell_chartab`]); for bytes 128..256 (`'encoding'`-
/// dependent), uses the real case-folding/upper-casing functions.
///
/// # Safety
/// Touches `crate::option_vars::OPTION_VARS` (via `mbyte::mb_toupper`/
/// `mb_isupper`/`mb_islower`).
pub unsafe fn init_spell_chartab() {
    unsafe {
        *DID_SET_SPELLTAB.get_mut() = false;
    }
    // SAFETY: a plain exclusive borrow of this file's own static.
    let sp = unsafe { SPELLTAB.get_mut() };
    clear_spell_chartab(sp);
    for i in 128i32..256 {
        let f = crate::mbyte::utf_fold(i);
        // SAFETY: forwarded from this function's own safety doc.
        let u = unsafe { crate::mbyte::mb_toupper(i) };
        // SAFETY: forwarded from this function's own safety doc.
        let isu = unsafe { crate::mbyte::mb_isupper(i) };
        // SAFETY: forwarded from this function's own safety doc.
        let isl = unsafe { crate::mbyte::mb_islower(i) };
        sp.st_isu[i as usize] = isu;
        sp.st_isw[i as usize] = isu || isl;
        sp.st_fold[i as usize] = if f < 256 { f as u8 } else { i as u8 };
        sp.st_upper[i as usize] = if u < 256 { u as u8 } else { i as u8 };
    }
}

/// `SPELL_ISUPPER(c)` (`mbyte.h`): whether codepoint `c` is uppercase,
/// per spelling's own chartab for `c < 128` (matching the original's
/// exact `c >= 128` cutoff - NOT 256, so bytes 128..255 always go
/// through the real `mb_isupper` even though [`SpelltabT`] itself
/// covers the full 0..256 range).
///
/// # Safety
/// If `c >= 128`, forwards `crate::mbyte::mb_isupper`'s own safety
/// doc (touches `crate::option_vars::OPTION_VARS`).
#[must_use]
pub unsafe fn spell_is_upper(c: i32) -> bool {
    if c >= 128 {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::mbyte::mb_isupper(c) }
    } else {
        // SAFETY: a plain read through one shared borrow of this
        // file's own static.
        unsafe { SPELLTAB.get_mut() }.st_isu[c as usize]
    }
}

/// `SPELL_TOFOLD(c)` (`mbyte.h`): the folded/lowercased equivalent of
/// codepoint `c`, per spelling's own chartab for `c < 128` (matching
/// the original's exact `c >= 128` cutoff).
///
/// A genuinely safe `fn` (unlike its sibling [`spell_is_upper`]/
/// [`spell_to_upper`]): `crate::mbyte::utf_fold` is itself a safe
/// function, and `SPELLTAB.get_mut()`'s own unsafety has no
/// caller-facing precondition (a plain read through one shared
/// borrow), matching `crate::option::get_ve_flags`'s own established
/// "safe wrapper around an internally-`unsafe`-but-precondition-free
/// global read" precedent.
#[must_use]
pub fn spell_to_fold(c: i32) -> i32 {
    if c >= 128 {
        crate::mbyte::utf_fold(c)
    } else {
        // SAFETY: a plain read through one shared borrow of this
        // file's own static - no caller-facing precondition.
        i32::from(unsafe { SPELLTAB.get_mut() }.st_fold[c as usize])
    }
}

/// `SPELL_TOUPPER(c)` (`mbyte.h`): the uppercase equivalent of
/// codepoint `c`, per spelling's own chartab for `c < 128` (matching
/// the original's exact `c >= 128` cutoff).
///
/// A genuinely safe `fn` (unlike [`spell_is_upper`]): `mb_toupper`'s
/// own unsafety is solely touching `crate::option_vars::OPTION_VARS`
/// (no precondition depending on `c` itself), so it's wrapped
/// internally here, matching `crate::option::get_findfunc`'s own
/// established "safe wrapper" precedent for this exact situation.
#[must_use]
pub fn spell_to_upper(c: i32) -> i32 {
    if c >= 128 {
        // SAFETY: `mb_toupper`'s only precondition is touching
        // `OPTION_VARS`, not anything depending on `c` - see this
        // function's own doc comment.
        unsafe { crate::mbyte::mb_toupper(c) }
    } else {
        // SAFETY: a plain read through one shared borrow of this
        // file's own static - no caller-facing precondition.
        i32::from(unsafe { SPELLTAB.get_mut() }.st_upper[c as usize])
    }
}

/// Returns `true` if `p` points to a word character. Unlike
/// `spell_iswordp`, this doesn't check for "midword" characters
/// (`spell_iswordp_nmw`).
///
/// # Safety
/// `p` must be non-empty and point to a valid, well-formed UTF-8 byte
/// sequence (forwarded from `crate::mbyte::utf_ptr2char`'s own safety
/// contract).
///
/// The `c > 255` branch is `unimplemented!()` - it needs
/// `mb_get_class`/`utf_class`, which need the REAL, populated
/// `curbuf->b_chartab` (the same `g_chartab` blocker documented
/// throughout `charset.rs` - needs `buf_T` plus real option parsing,
/// not translated). No current caller in this crate can pass
/// genuinely non-Latin-1 input yet (this function itself has no real
/// translated caller today either - see `captype`'s own doc comment).
#[must_use]
pub unsafe fn spell_iswordp_nmw(p: &[u8]) -> bool {
    let c = crate::mbyte::utf_ptr2char(p);
    if c > 255 {
        unimplemented!(
            "spell_iswordp_nmw: c > 255 needs mb_get_class/utf_class -> curbuf.b_chartab, not translated"
        );
    }
    // SAFETY: a plain read through one shared borrow of this file's
    // own static.
    unsafe { SPELLTAB.get_mut() }.st_isw[c as usize]
}

/// Returns the case type of `word[..end]` (or, if `end` is `None`,
/// `word` up to its own trailing NUL) - one of [`WF_ALLCAP`]/
/// [`WF_ONECAP`]/[`WF_KEEPCAP`]/`0` (plain) (`captype`).
///
/// # Safety
/// `word` must be non-empty and, if `end.is_none()`, NUL-terminated
/// (this crate's own established line-buffer convention) - forwarded
/// from [`spell_iswordp_nmw`]'s own safety doc, which this function
/// calls throughout.
///
/// Panics (via [`spell_iswordp_nmw`]) if `word` contains a genuinely
/// non-Latin-1 (`c > 255`) character before `end` - see that
/// function's own doc comment. Has no real translated caller itself
/// yet either (`find_word`, needing `slang_T`'s own spell-file-loaded
/// state, not translated) - translated ahead of it anyway, matching
/// this crate's established "small, self-contained, no design freedom
/// to get wrong" precedent, since the algorithm itself has none.
#[must_use]
pub unsafe fn captype(word: &[u8], end: Option<usize>) -> i32 {
    let reached_end = |pos: usize| match end {
        None => pos >= word.len() || word[pos] == 0,
        Some(e) => pos >= e,
    };

    // Find the first word character.
    let mut pos = 0usize;
    loop {
        // SAFETY: forwarded from this function's own safety doc -
        // `pos < word.len()` is an invariant of this loop (see below).
        if unsafe { spell_iswordp_nmw(&word[pos..]) } {
            break;
        }
        if reached_end(pos) {
            return 0; // only non-word characters, illegal word
        }
        // SAFETY: forwarded from this function's own safety doc.
        let adv = usize::try_from(unsafe { crate::mbyte::utfc_ptr2len(&word[pos..]) }).unwrap_or(0);
        pos += adv.max(1).min(word.len() - pos);
    }

    // SAFETY: forwarded from this function's own safety doc.
    let (mut c, consumed) = unsafe { crate::mbyte::mb_ptr2char_adv(&word[pos..]) };
    pos += consumed.max(1).min(word.len() - pos);
    // SAFETY: forwarded from this function's own safety doc.
    let firstcap = unsafe { spell_is_upper(c) };
    let mut allcap = firstcap;
    let mut past_second = false;

    // Need to check all letters to find a word with mixed upper/
    // lower. But a word with an upper char only at start is a ONECAP.
    while !reached_end(pos) {
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { spell_iswordp_nmw(&word[pos..]) } {
            c = crate::mbyte::utf_ptr2char(&word[pos..]);
            // SAFETY: forwarded from this function's own safety doc.
            if !unsafe { spell_is_upper(c) } {
                // UUl -> KEEPCAP
                if past_second && allcap {
                    return WF_KEEPCAP;
                }
                allcap = false;
            } else if !allcap {
                // UlU -> KEEPCAP
                return WF_KEEPCAP;
            }
            past_second = true;
        }
        // SAFETY: forwarded from this function's own safety doc.
        let adv = usize::try_from(unsafe { crate::mbyte::utfc_ptr2len(&word[pos..]) }).unwrap_or(0);
        pos += adv.max(1).min(word.len() - pos);
    }

    if allcap {
        return WF_ALLCAP;
    }
    if firstcap {
        return WF_ONECAP;
    }
    0
}

/// Return the encoding used for spell checking: `'encoding'`, except
/// that `"latin1"` is used for `"iso-8859-15"`, and limited to 60
/// characters (just in case) (`spell_enc`).
///
/// Returns an owned `Vec<u8>` instead of the original's `char *`
/// (aliasing `p_enc` directly, or a static `"latin1"` string literal) -
/// no lifetime hazard to work around in Rust. Matches
/// `fileio.rs`'s own `get_fio_flags` precedent for reading this exact
/// field: `p_enc == None` (this crate's own `Default` mirrors raw C
/// zero-init, not the real, always-populated `ENC_DFLT` value a
/// genuine running session has) falls back to an empty `Vec<u8>` via
/// `unwrap_or_default()`, not `ENC_DFLT` itself - a real running
/// session's `p_enc` is never actually unset.
#[must_use]
pub fn spell_enc() -> Vec<u8> {
    // SAFETY: only reads a plain `Option<Vec<u8>>` field, no raw
    // pointers/aliasing involved.
    let p_enc = unsafe { crate::option_vars::OPTION_VARS.get_mut() }
        .p_enc
        .clone()
        .unwrap_or_default();
    if p_enc.len() < 60 && p_enc != b"iso-8859-15" {
        p_enc
    } else {
        b"latin1".to_vec()
    }
}

/// Case-folding may change the number of bytes: count the number of
/// characters in `fword[..flen]` and return the byte length of that
/// many characters in `word` (`nofold_len`).
///
/// Deviates from the original's raw pointer-difference return (always
/// non-negative here, since `word` is only ever walked forward from
/// its own start) by returning a `usize` byte offset directly, matching
/// this crate's established "index instead of pointer" convention.
///
/// This is a genuinely safe `fn` (unlike its sibling `unsafe fn`s in
/// this file): `crate::mbyte::utfc_ptr2len` itself never reads out of
/// bounds for ANY byte content (it bounds its own scan to the slice's
/// own length, verified directly in its own implementation), so no
/// caller-side safety precondition is needed here, matching
/// `crate::spellfile::valid_spell_word`'s own established precedent
/// for this exact situation.
#[must_use]
pub fn nofold_len(fword: &[u8], flen: usize, word: &[u8]) -> usize {
    let mut n = 0usize;
    let mut p = 0usize;
    while p < flen {
        // SAFETY: `utfc_ptr2len` never reads out of bounds for any
        // byte content - see this function's own doc comment.
        let adv = usize::try_from(unsafe { crate::mbyte::utfc_ptr2len(&fword[p..]) }).unwrap_or(0);
        // .min(...) is a defensive net against a caller-supplied
        // `flen` exceeding `fword.len()` (a contract violation the
        // original C also implicitly assumes never happens) - never
        // changes behavior for well-formed input.
        p += adv.max(1).min(fword.len().saturating_sub(p));
        n += 1;
    }
    let mut q = 0usize;
    while n > 0 {
        // SAFETY: forwarded from this function's own doc comment.
        let adv = usize::try_from(unsafe { crate::mbyte::utfc_ptr2len(&word[q..]) }).unwrap_or(0);
        q += adv.max(1).min(word.len().saturating_sub(q));
        n -= 1;
    }
    q
}

/// Make a copy of `word`, with the first letter upper- or lower-cased
/// (`onecap_copy`). `word` must not be empty. The result includes its
/// own trailing NUL, matching this crate's established convention for
/// freshly-produced string outputs (e.g. `crate::strings::strcase_save`).
///
/// Deviates from the original's `wcopy[MAXWLEN]` fixed-size,
/// truncating output buffer by returning an unbounded, growing
/// `Vec<u8>` instead - no caller yet depends on the `MAXWLEN`
/// truncation itself (a defensive C buffer-overflow guard for
/// pathologically long input, not meaningful editing behavior),
/// matching this crate's established "growing `Vec` supersedes the
/// manual bounded-buffer C idiom" precedent (`winrestcmd`/
/// `vim_strsave_shellescape`).
#[must_use]
pub fn onecap_copy(word: &[u8], upper: bool) -> Vec<u8> {
    let (c, consumed) = crate::mbyte::mb_cptr2char_adv(word);
    let c = if upper { spell_to_upper(c) } else { spell_to_fold(c) };
    let mut buf = [0u8; crate::mbyte_defs::MB_MAXCHAR + 1];
    let l = crate::mbyte::utf_char2bytes(c, &mut buf) as usize;
    let mut result = buf[..l].to_vec();
    let rest = &word[consumed.min(word.len())..];
    let rest_len = rest.iter().position(|&b| b == 0).unwrap_or(rest.len());
    result.extend_from_slice(&rest[..rest_len]);
    result.push(0);
    result
}

/// Make a copy of `word` with all the letters upper-cased
/// (`allcap_copy`). The result includes its own trailing NUL, matching
/// this crate's established convention for freshly-produced string
/// outputs. Deviates from the original's `wcopy[MAXWLEN]` fixed-size,
/// truncating output buffer the same way as [`onecap_copy`] - see its
/// own doc comment for why.
///
/// Faithfully preserves a real, deliberate original quirk: German
/// sharp s (`ß`, U+00DF) uppercases to `"SS"` (TWO characters), not a
/// single character - the original's own code writes `'S'` directly,
/// then ALSO falls through to the shared `utf_char2bytes` call with
/// `c` still `'S'`, writing it a second time. Verified this is real
/// German uppercasing behavior, not a translation bug.
#[must_use]
pub fn allcap_copy(word: &[u8]) -> Vec<u8> {
    let mut result = Vec::new();
    let mut s = 0usize;
    while s < word.len() && word[s] != 0 {
        let (c, consumed) = crate::mbyte::mb_cptr2char_adv(&word[s..]);
        s += consumed.max(1).min(word.len() - s);

        let mut buf = [0u8; crate::mbyte_defs::MB_MAXCHAR + 1];
        if c == 0xdf {
            let l = crate::mbyte::utf_char2bytes(i32::from(b'S'), &mut buf) as usize;
            result.extend_from_slice(&buf[..l]);
            result.extend_from_slice(&buf[..l]);
        } else {
            let up = spell_to_upper(c);
            let l = crate::mbyte::utf_char2bytes(up, &mut buf) as usize;
            result.extend_from_slice(&buf[..l]);
        }
    }
    result.push(0);
    result
}

/// Copy `fword` to a new word, fixing case according to `flags`
/// (`make_case_word`). `flags` is checked against [`WF_ALLCAP`] first,
/// then [`WF_ONECAP`]; otherwise `fword` is returned as-is (still
/// including its own trailing NUL, matching this crate's established
/// convention).
#[must_use]
pub fn make_case_word(fword: &[u8], flags: i32) -> Vec<u8> {
    if flags & WF_ALLCAP != 0 {
        allcap_copy(fword)
    } else if flags & WF_ONECAP != 0 {
        onecap_copy(fword, true)
    } else {
        let len = fword.iter().position(|&b| b == 0).unwrap_or(fword.len());
        let mut result = fword[..len].to_vec();
        result.push(0);
        result
    }
}

/// Whether byte value `n` appears anywhere in `s` (`byte_in_str`).
///
/// Like `strchr`, but independent of locale, per the original's own
/// doc comment.
#[must_use]
pub fn byte_in_str(s: &[u8], n: i32) -> bool {
    s.iter().any(|&b| i32::from(b) == n)
}

/// Checks case flags for a word - `true` if the word (`wordflags`) has
/// the case required by its spell-tree entry (`treeflags`)
/// (`spell_valid_case`).
#[must_use]
pub fn spell_valid_case(wordflags: i32, treeflags: i32) -> bool {
    (wordflags == WF_ALLCAP && (treeflags & WF_FIXCAP) == 0)
        || ((treeflags & (WF_ALLCAP | WF_KEEPCAP)) == 0
            && ((treeflags & WF_ONECAP) == 0 || (wordflags & WF_ONECAP) != 0))
}

/// Finds `region` (exactly 2 bytes) within `rp` (a sequence of 2-byte
/// region codes), returning its 0-based index (counting in 2-byte
/// units) if found, or [`REGION_ALL`] if not (`find_region`).
#[must_use]
pub fn find_region(rp: &[u8], region: [u8; 2]) -> i32 {
    let mut i = 0;
    loop {
        if i >= rp.len() {
            return REGION_ALL;
        }
        if rp[i] == region[0] && rp.get(i + 1) == Some(&region[1]) {
            return (i / 2) as i32;
        }
        i += 2;
    }
}

/// Whether `val` is a valid `'spelllang'` value (`valid_spelllang`).
#[must_use]
pub fn valid_spelllang(val: &[u8]) -> bool {
    crate::option::valid_name(val, b".-_,@")
}

/// Whether `val` is a valid `'spellfile'` value (`valid_spellfile`):
/// every comma-separated part must be a `.add`-suffixed name made up
/// entirely of valid filename characters.
#[must_use]
pub fn valid_spellfile(val: &[u8]) -> bool {
    let maxpathl = crate::os::os_defs::MAXPATHL as usize;
    let mut p = 0;
    while p < val.len() {
        let (spf_name, next_p) = crate::option::copy_option_part(val, p, maxpathl, b",");
        p = next_p;
        let l = spf_name.len();
        if l >= maxpathl - 4 || l < 4 || spf_name[l - 4..] != *b".add" {
            return false;
        }
        if !spf_name.iter().all(|&c| crate::charset::vim_is_fname_char(i32::from(c))) {
            return false;
        }
    }
    true
}

/// Whether spell checking is enabled for `wp` (`spell_check_window`):
/// `'spell'` is set AND a `'spelllang'` value is loaded.
///
/// # Safety
/// `wp.w_s` must be a valid, non-null pointer to a live
/// `crate::buffer_defs::SynblockT`.
#[must_use]
pub unsafe fn spell_check_window(wp: &crate::buffer_defs::WinT) -> bool {
    wp.w_onebuf_opt.wo_spell != 0
        // SAFETY: forwarded from this function's own safety doc.
        && unsafe { &*wp.w_s }.b_p_spl.as_deref().is_some_and(|v| !v.is_empty())
}

/// Whether spell checking is disabled (or no spell language is
/// actually loaded) for `wp` (`no_spell_checking`). The original's own
/// `emsg(_(e_no_spell))` display is skipped (`message.c`'s display
/// pipeline is not tractable), matching this crate's established
/// policy elsewhere - only the boolean OUTCOME is preserved.
///
/// # Safety
/// Same as [`spell_check_window`].
#[must_use]
pub unsafe fn no_spell_checking(wp: &crate::buffer_defs::WinT) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    if !unsafe { spell_check_window(wp) } || unsafe { &*wp.w_s }.b_langp.is_empty() {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_in_str_finds_present_byte() {
        assert!(byte_in_str(b"hello", i32::from(b'e')));
        assert!(byte_in_str(b"hello", i32::from(b'o')));
    }

    #[test]
    fn byte_in_str_missing_byte_is_false() {
        assert!(!byte_in_str(b"hello", i32::from(b'z')));
        assert!(!byte_in_str(b"", i32::from(b'a')));
    }

    #[test]
    fn spell_valid_case_allcap_word_matches_unless_fixcap() {
        assert!(spell_valid_case(WF_ALLCAP, 0));
        // WF_FIXCAP alone in treeflags doesn't block a match here: the
        // first disjunct fails (fixcap is set), but the second disjunct
        // is satisfied anyway since treeflags has neither ALLCAP nor
        // KEEPCAP nor ONECAP set. FIXCAP only actually blocks a match
        // when treeflags ALSO has ALLCAP or KEEPCAP set (verified by
        // hand-deriving the real bitwise formula after this assertion
        // first failed unexpectedly).
        assert!(spell_valid_case(WF_ALLCAP, WF_FIXCAP));
        assert!(!spell_valid_case(WF_ALLCAP, WF_FIXCAP | WF_ALLCAP));
    }

    #[test]
    fn spell_valid_case_plain_word_matches_plain_tree_entry() {
        assert!(spell_valid_case(0, 0));
    }

    #[test]
    fn spell_valid_case_onecap_word_matches_onecap_tree_entry() {
        assert!(spell_valid_case(WF_ONECAP, WF_ONECAP));
        // A plain (lowercase) word does NOT match a ONECAP-required entry.
        assert!(!spell_valid_case(0, WF_ONECAP));
    }

    #[test]
    fn spell_valid_case_rejects_allcap_or_keepcap_tree_entries_otherwise() {
        assert!(!spell_valid_case(0, WF_ALLCAP));
        assert!(!spell_valid_case(0, WF_KEEPCAP));
    }

    #[test]
    fn find_region_finds_exact_match() {
        assert_eq!(find_region(b"usgbde", *b"us"), 0);
        assert_eq!(find_region(b"usgbde", *b"gb"), 1);
        assert_eq!(find_region(b"usgbde", *b"de"), 2);
    }

    #[test]
    fn find_region_not_found_is_region_all() {
        assert_eq!(find_region(b"usgbde", *b"xx"), REGION_ALL);
        assert_eq!(find_region(b"", *b"us"), REGION_ALL);
    }

    // --- clear_spell_chartab / init_spell_chartab ---

    #[test]
    fn clear_spell_chartab_digits_are_word_chars() {
        let mut sp = SpelltabT::default();
        clear_spell_chartab(&mut sp);
        assert!(sp.st_isw[b'5' as usize]);
        assert!(!sp.st_isw[b' ' as usize]);
    }

    #[test]
    fn clear_spell_chartab_uppercase_letters_are_word_and_upper() {
        let mut sp = SpelltabT::default();
        clear_spell_chartab(&mut sp);
        assert!(sp.st_isw[b'A' as usize]);
        assert!(sp.st_isu[b'A' as usize]);
        assert_eq!(sp.st_fold[b'A' as usize], b'a');
    }

    #[test]
    fn clear_spell_chartab_lowercase_letters_are_word_not_upper() {
        let mut sp = SpelltabT::default();
        clear_spell_chartab(&mut sp);
        assert!(sp.st_isw[b'a' as usize]);
        assert!(!sp.st_isu[b'a' as usize]);
        assert_eq!(sp.st_upper[b'a' as usize], b'A');
    }

    #[test]
    fn clear_spell_chartab_non_letter_gets_identity_fold_and_upper() {
        let mut sp = SpelltabT::default();
        clear_spell_chartab(&mut sp);
        assert!(!sp.st_isw[b' ' as usize]);
        assert_eq!(sp.st_fold[b' ' as usize], b' ');
        assert_eq!(sp.st_upper[b' ' as usize], b' ');
    }

    #[test]
    fn clear_spell_chartab_resets_a_previously_dirty_table() {
        // A table that was previously populated by init_spell_chartab
        // (or hand-corrupted) is fully reset, not just "added to".
        let mut sp = SpelltabT { st_isw: [true; 256], ..SpelltabT::default() };
        clear_spell_chartab(&mut sp);
        assert!(!sp.st_isw[b'!' as usize]);
    }

    #[test]
    fn init_spell_chartab_resets_did_set_spelltab_and_matches_clear_for_ascii() {
        let _lock = crate::globals::global_state_test_lock();
        unsafe { *DID_SET_SPELLTAB.get_mut() = true };

        unsafe { init_spell_chartab() };

        assert!(!unsafe { *DID_SET_SPELLTAB.get_mut() });
        // Bytes 0..128 are untouched by init_spell_chartab's own loop
        // (which only runs 128..256) - they must still match a plain
        // clear_spell_chartab call exactly.
        let sp = unsafe { SPELLTAB.get_mut() };
        let mut expected = SpelltabT::default();
        clear_spell_chartab(&mut expected);
        assert_eq!(&sp.st_isw[0..128], &expected.st_isw[0..128]);
        assert_eq!(&sp.st_fold[0..128], &expected.st_fold[0..128]);
    }

    // --- spell_is_upper / spell_iswordp_nmw / captype ---

    /// Resets the shared `SPELLTAB` to its deterministic ASCII
    /// baseline (via `clear_spell_chartab` alone, NOT the full
    /// `init_spell_chartab` - avoiding that function's own Miri-FFI-
    /// incompatible bytes-128..256 loop, unneeded for these
    /// ASCII-only test scenarios). Caller must hold
    /// `global_state_test_lock()`.
    fn reset_spelltab_ascii() {
        clear_spell_chartab(unsafe { SPELLTAB.get_mut() });
    }

    #[test]
    fn spell_is_upper_ascii_uses_spelltab() {
        let _lock = crate::globals::global_state_test_lock();
        reset_spelltab_ascii();
        assert!(unsafe { spell_is_upper(i32::from(b'A')) });
        assert!(!unsafe { spell_is_upper(i32::from(b'a')) });
        assert!(!unsafe { spell_is_upper(i32::from(b'5')) });
    }

    #[test]
    fn spell_iswordp_nmw_letters_and_digits_are_word_chars() {
        let _lock = crate::globals::global_state_test_lock();
        reset_spelltab_ascii();
        assert!(unsafe { spell_iswordp_nmw(b"a") });
        assert!(unsafe { spell_iswordp_nmw(b"5") });
        assert!(!unsafe { spell_iswordp_nmw(b" ") });
    }

    #[test]
    fn captype_all_lowercase_is_plain() {
        let _lock = crate::globals::global_state_test_lock();
        reset_spelltab_ascii();
        assert_eq!(unsafe { captype(b"hello\0", None) }, 0);
    }

    #[test]
    fn captype_all_uppercase_is_allcap() {
        let _lock = crate::globals::global_state_test_lock();
        reset_spelltab_ascii();
        assert_eq!(unsafe { captype(b"HELLO\0", None) }, WF_ALLCAP);
    }

    #[test]
    fn captype_leading_capital_only_is_onecap() {
        let _lock = crate::globals::global_state_test_lock();
        reset_spelltab_ascii();
        assert_eq!(unsafe { captype(b"Hello\0", None) }, WF_ONECAP);
    }

    #[test]
    fn captype_uul_pattern_is_keepcap() {
        // "HEllo": two leading capitals then lowercase - hand-traced
        // against the original's own "UUl -> KEEPCAP" comment: after
        // 'H' (pos=0, firstcap=allcap=true) and 'E' (still allcap,
        // past_second becomes true), 'l' triggers
        // `past_second && allcap` -> KEEPCAP.
        let _lock = crate::globals::global_state_test_lock();
        reset_spelltab_ascii();
        assert_eq!(unsafe { captype(b"HEllo\0", None) }, WF_KEEPCAP);
    }

    #[test]
    fn captype_ulu_pattern_is_keepcap() {
        // "HeLLo": capital, then lowercase, then capital again -
        // hand-traced against the original's own "UlU -> KEEPCAP"
        // comment: after 'H' (firstcap=allcap=true) and 'e' (allcap
        // becomes false, past_second becomes true), 'L' triggers the
        // `else if (!allcap)` branch -> KEEPCAP.
        let _lock = crate::globals::global_state_test_lock();
        reset_spelltab_ascii();
        assert_eq!(unsafe { captype(b"HeLLo\0", None) }, WF_KEEPCAP);
    }

    #[test]
    fn captype_only_non_word_characters_is_zero() {
        let _lock = crate::globals::global_state_test_lock();
        reset_spelltab_ascii();
        assert_eq!(unsafe { captype(b"   \0", None) }, 0);
    }

    #[test]
    fn captype_respects_an_explicit_end_offset() {
        // "HELLOworld" with end=5 only looks at "HELLO" - still
        // WF_ALLCAP, ignoring the lowercase "world" that follows.
        let _lock = crate::globals::global_state_test_lock();
        reset_spelltab_ascii();
        assert_eq!(unsafe { captype(b"HELLOworld", Some(5)) }, WF_ALLCAP);
    }

    #[test]
    fn captype_leading_punctuation_is_skipped() {
        // A leading non-word character is skipped when finding the
        // first word character - the case-type is based on "Hello"
        // itself, not the punctuation before it.
        let _lock = crate::globals::global_state_test_lock();
        reset_spelltab_ascii();
        assert_eq!(unsafe { captype(b"'Hello\0", None) }, WF_ONECAP);
    }

    // --- spell_enc ---

    #[test]
    fn spell_enc_none_falls_back_to_empty_not_enc_dflt() {
        // p_enc == None (this crate's own raw zero-init default, never
        // actually seen in a real running session) falls back to an
        // empty Vec<u8>, matching fileio.rs's own get_fio_flags
        // precedent for this exact field - NOT crate::option_vars::
        // ENC_DFLT ("utf-8"), even though that's the real compiled
        // default a genuine session always has.
        let _lock = crate::globals::global_state_test_lock();
        let saved = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_enc.clone();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_enc = None;
        let result = spell_enc();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_enc = saved;
        assert_eq!(result, Vec::<u8>::new());
    }

    #[test]
    fn spell_enc_returns_p_enc_when_short_and_not_iso_8859_15() {
        let _lock = crate::globals::global_state_test_lock();
        let saved = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_enc.clone();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_enc = Some(b"utf-8".to_vec());
        let result = spell_enc();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_enc = saved;
        assert_eq!(result, b"utf-8");
    }

    #[test]
    fn spell_enc_iso_8859_15_maps_to_latin1() {
        let _lock = crate::globals::global_state_test_lock();
        let saved = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_enc.clone();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_enc = Some(b"iso-8859-15".to_vec());
        let result = spell_enc();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_enc = saved;
        assert_eq!(result, b"latin1");
    }

    #[test]
    fn spell_enc_60_or_more_bytes_maps_to_latin1() {
        // strlen(p_enc) < 60 is a strict less-than: exactly 60 bytes
        // already falls through to "latin1".
        let _lock = crate::globals::global_state_test_lock();
        let saved = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_enc.clone();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_enc = Some(vec![b'x'; 60]);
        let result = spell_enc();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_enc = saved;
        assert_eq!(result, b"latin1");
    }

    #[test]
    fn spell_enc_59_bytes_is_returned_as_is() {
        let _lock = crate::globals::global_state_test_lock();
        let saved = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_enc.clone();
        let enc = vec![b'x'; 59];
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_enc = Some(enc.clone());
        let result = spell_enc();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_enc = saved;
        assert_eq!(result, enc);
    }

    // --- nofold_len ---

    #[test]
    fn nofold_len_all_ascii_matches_byte_for_byte() {
        // "abc" folded to "abc" (no change): 3 chars in fword[0..3]
        // maps to byte offset 3 in word.
        assert_eq!(nofold_len(b"abc", 3, b"ABC"), 3);
    }

    #[test]
    fn nofold_len_zero_flen_is_zero_chars_is_zero_bytes() {
        assert_eq!(nofold_len(b"abc", 0, b"ABC"), 0);
    }

    #[test]
    fn nofold_len_multibyte_word_folds_to_fewer_bytes() {
        // word = "日bc" (3 chars: U+65E5 is 3 bytes, 'b'/'c' 1 byte
        // each = 5 bytes total). Artificially treat it as if it folds
        // to fword = "xbc" (3 chars, 3 bytes - a single ASCII stand-in
        // for whatever U+65E5 folds to, independent of real Unicode
        // folding rules, to isolate the char-count-vs-byte-length
        // algorithm itself). flen=2 selects "xb" (2 chars) from fword;
        // the same 2 characters in word are "日b" = 3+1 = 4 bytes.
        let word = "日bc".as_bytes();
        assert_eq!(nofold_len(b"xbc", 2, word), 4);
    }

    #[test]
    fn nofold_len_entire_fword_consumed() {
        // flen covers the whole 3-char fword; word's own first 3
        // chars are "日bc" in full (3+1+1 = 5 bytes).
        let word = "日bc".as_bytes();
        assert_eq!(nofold_len(b"xbc", 3, word), word.len());
    }

    // --- spell_to_fold / spell_to_upper ---

    #[test]
    fn spell_to_fold_ascii_uses_spelltab() {
        let _lock = crate::globals::global_state_test_lock();
        reset_spelltab_ascii();
        assert_eq!(spell_to_fold(i32::from(b'A')), i32::from(b'a'));
        assert_eq!(spell_to_fold(i32::from(b'a')), i32::from(b'a'));
        assert_eq!(spell_to_fold(i32::from(b'5')), i32::from(b'5'));
    }

    #[test]
    fn spell_to_upper_ascii_uses_spelltab() {
        let _lock = crate::globals::global_state_test_lock();
        reset_spelltab_ascii();
        assert_eq!(spell_to_upper(i32::from(b'a')), i32::from(b'A'));
        assert_eq!(spell_to_upper(i32::from(b'A')), i32::from(b'A'));
        assert_eq!(spell_to_upper(i32::from(b'5')), i32::from(b'5'));
    }

    // --- onecap_copy / allcap_copy / make_case_word ---

    #[test]
    fn onecap_copy_uppercases_only_the_first_letter() {
        let _lock = crate::globals::global_state_test_lock();
        reset_spelltab_ascii();
        assert_eq!(onecap_copy(b"hello", true), b"Hello\0");
    }

    #[test]
    fn onecap_copy_folds_only_the_first_letter() {
        // upper=false folds (lower-cases) just the first letter,
        // leaving the rest of the word untouched - "HELLO" ->
        // "hELLO", not "hello".
        let _lock = crate::globals::global_state_test_lock();
        reset_spelltab_ascii();
        assert_eq!(onecap_copy(b"HELLO", false), b"hELLO\0");
    }

    #[test]
    fn onecap_copy_already_correct_case_is_a_no_op() {
        let _lock = crate::globals::global_state_test_lock();
        reset_spelltab_ascii();
        assert_eq!(onecap_copy(b"Hello", true), b"Hello\0");
    }

    #[test]
    fn allcap_copy_uppercases_every_letter() {
        let _lock = crate::globals::global_state_test_lock();
        reset_spelltab_ascii();
        assert_eq!(allcap_copy(b"hello"), b"HELLO\0");
    }

    #[test]
    fn allcap_copy_sharp_s_becomes_two_esses() {
        // German sharp s (U+00DF, 2-byte UTF-8: 0xC3 0x9F) uppercases
        // to "SS" (two characters) - a real, deliberate original
        // quirk (see allcap_copy's own doc comment), not a
        // translation bug.
        let _lock = crate::globals::global_state_test_lock();
        reset_spelltab_ascii();
        let word = "aß".as_bytes();
        assert_eq!(allcap_copy(word), b"ASS\0");
    }

    #[test]
    fn allcap_copy_stops_at_the_first_embedded_nul() {
        let _lock = crate::globals::global_state_test_lock();
        reset_spelltab_ascii();
        assert_eq!(allcap_copy(b"ab\0cd"), b"AB\0");
    }

    #[test]
    fn make_case_word_allcap_flag_uppercases_everything() {
        let _lock = crate::globals::global_state_test_lock();
        reset_spelltab_ascii();
        assert_eq!(make_case_word(b"hello", WF_ALLCAP), b"HELLO\0");
    }

    #[test]
    fn make_case_word_onecap_flag_uppercases_only_the_first_letter() {
        let _lock = crate::globals::global_state_test_lock();
        reset_spelltab_ascii();
        assert_eq!(make_case_word(b"hello", WF_ONECAP), b"Hello\0");
    }

    #[test]
    fn make_case_word_no_flags_is_used_as_is() {
        let _lock = crate::globals::global_state_test_lock();
        reset_spelltab_ascii();
        assert_eq!(make_case_word(b"hello", 0), b"hello\0");
    }

    #[test]
    fn make_case_word_allcap_takes_priority_over_onecap() {
        // Matches the original's own `if (flags & WF_ALLCAP) ... else
        // if (flags & WF_ONECAP)` order - ALLCAP wins when both bits
        // happen to be set.
        let _lock = crate::globals::global_state_test_lock();
        reset_spelltab_ascii();
        assert_eq!(make_case_word(b"hello", WF_ALLCAP | WF_ONECAP), b"HELLO\0");
    }

    // --- valid_spelllang ---

    #[test]
    fn valid_spelllang_empty_is_valid() {
        assert!(valid_spelllang(b""));
    }

    #[test]
    fn valid_spelllang_plain_alnum_is_valid() {
        assert!(valid_spelllang(b"en"));
    }

    #[test]
    fn valid_spelllang_allowed_punctuation_is_valid() {
        assert!(valid_spelllang(b"en_US.utf-8,de@quot"));
    }

    #[test]
    fn valid_spelllang_space_is_invalid() {
        assert!(!valid_spelllang(b"en US"));
    }

    // --- valid_spellfile ---

    #[test]
    fn valid_spellfile_empty_is_valid() {
        assert!(valid_spellfile(b""));
    }

    #[test]
    fn valid_spellfile_single_add_suffixed_part_is_valid() {
        assert!(valid_spellfile(b"en.utf-8.add"));
    }

    #[test]
    fn valid_spellfile_multiple_comma_separated_parts_all_valid() {
        assert!(valid_spellfile(b"en.add,de.add"));
    }

    #[test]
    fn valid_spellfile_bare_dot_add_is_valid() {
        // l == 4 exactly: not "< 4" (rejected), the whole part IS the
        // ".add" suffix with nothing before it - matches the
        // original's own l < 4 (strictly-less) check precisely.
        assert!(valid_spellfile(b".add"));
    }

    #[test]
    fn valid_spellfile_too_short_is_invalid() {
        assert!(!valid_spellfile(b"abc"));
    }

    #[test]
    fn valid_spellfile_wrong_suffix_is_invalid() {
        assert!(!valid_spellfile(b"foo.txt"));
    }

    #[test]
    fn valid_spellfile_one_bad_part_among_several_invalidates_the_whole_value() {
        assert!(!valid_spellfile(b"en.add,invalid"));
    }

    #[test]
    fn valid_spellfile_invalid_filename_character_is_rejected() {
        assert!(!valid_spellfile(b"a<b.add"));
    }

    // --- spell_check_window / no_spell_checking ---

    #[test]
    fn spell_check_window_true_when_spell_on_and_lang_set() {
        let mut win = crate::buffer_defs::WinT::default();
        win.w_onebuf_opt.wo_spell = 1;
        let mut syn = crate::buffer_defs::SynblockT { b_p_spl: Some(b"en".to_vec()), ..Default::default() };
        win.w_s = &mut syn as *mut crate::buffer_defs::SynblockT;

        assert!(unsafe { spell_check_window(&win) });
    }

    #[test]
    fn spell_check_window_false_when_spell_option_off() {
        let mut win = crate::buffer_defs::WinT::default();
        win.w_onebuf_opt.wo_spell = 0;
        let mut syn = crate::buffer_defs::SynblockT { b_p_spl: Some(b"en".to_vec()), ..Default::default() };
        win.w_s = &mut syn as *mut crate::buffer_defs::SynblockT;

        assert!(!unsafe { spell_check_window(&win) });
    }

    #[test]
    fn spell_check_window_false_when_spelllang_is_unset() {
        let mut win = crate::buffer_defs::WinT::default();
        win.w_onebuf_opt.wo_spell = 1;
        let mut syn = crate::buffer_defs::SynblockT { b_p_spl: None, ..Default::default() };
        win.w_s = &mut syn as *mut crate::buffer_defs::SynblockT;

        assert!(!unsafe { spell_check_window(&win) });
    }

    #[test]
    fn spell_check_window_false_when_spelllang_is_empty_string() {
        let mut win = crate::buffer_defs::WinT::default();
        win.w_onebuf_opt.wo_spell = 1;
        let mut syn = crate::buffer_defs::SynblockT { b_p_spl: Some(Vec::new()), ..Default::default() };
        win.w_s = &mut syn as *mut crate::buffer_defs::SynblockT;

        assert!(!unsafe { spell_check_window(&win) });
    }

    #[test]
    fn no_spell_checking_false_when_enabled_and_a_language_is_loaded() {
        let mut win = crate::buffer_defs::WinT::default();
        win.w_onebuf_opt.wo_spell = 1;
        let mut syn = crate::buffer_defs::SynblockT {
            b_p_spl: Some(b"en".to_vec()),
            b_langp: crate::garray_defs::GarrayT { ga_len: 1, ..Default::default() },
            ..Default::default()
        };
        win.w_s = &mut syn as *mut crate::buffer_defs::SynblockT;

        assert!(!unsafe { no_spell_checking(&win) });
    }

    #[test]
    fn no_spell_checking_true_when_spell_checking_itself_is_disabled() {
        let mut win = crate::buffer_defs::WinT::default();
        win.w_onebuf_opt.wo_spell = 0;
        let mut syn = crate::buffer_defs::SynblockT { b_p_spl: Some(b"en".to_vec()), ..Default::default() };
        win.w_s = &mut syn as *mut crate::buffer_defs::SynblockT;

        assert!(unsafe { no_spell_checking(&win) });
    }

    #[test]
    fn no_spell_checking_true_when_no_language_is_actually_loaded() {
        // spell_check_window is true (spell on, a 'spelllang' value is
        // set), but b_langp (the ACTUALLY LOADED languages) is empty -
        // matching the real-world "spelllang set but the file hasn't
        // loaded yet/failed to load" case.
        let mut win = crate::buffer_defs::WinT::default();
        win.w_onebuf_opt.wo_spell = 1;
        let mut syn = crate::buffer_defs::SynblockT { b_p_spl: Some(b"en".to_vec()), ..Default::default() };
        win.w_s = &mut syn as *mut crate::buffer_defs::SynblockT;

        assert!(unsafe { no_spell_checking(&win) });
    }
}
