//! Translated from `src/nvim/diff.c` (tractable core only).
//!
//! `diff.c` (~3000 lines) is neovim's diff-mode engine (computing/
//! displaying/navigating diff hunks between buffers) - a substantial
//! subsystem of its own, almost entirely dependent on real diff
//! computation (internal xdiff or external `diff` invocation), not
//! attempted here.
//!
//! Translated: [`DIFF_FLAGS`] (the file-static `diff_flags` bitset,
//! translated with its own exact real default-initializer value -
//! `DIFF_INTERNAL | DIFF_FILLER | DIFF_CLOSE_OFF | DIFF_LINEMATCH |
//! DIFF_INLINE_CHAR`, matching the real `'diffopt'` default string
//! `"internal,filler,closeoff,indent-heuristic,inline:char,
//! linematch:40"` - `indent-heuristic`/`linematch:40` affect other,
//! not-yet-translated file-statics, not `diff_flags` itself), the
//! `DIFF_*` flag constants, [`diffopt_filler`]/[`diffopt_closeoff`]/
//! [`diffopt_horizontal`]/[`diffopt_hiddenoff`] (pure bit tests);
//! [`diff_check_with_linestatus`]/
//! [`diff_check_fill`] - real, faithful translations of their "no
//! diffs at all in this tab page" early-return path (`curtab.
//! tp_first_diff.is_null()`, always true today since nothing in this
//! crate can create a diff - `:diffthis`/diff-computation machinery
//! not translated), matching this session's established "translate
//! the real always-taken early-return condition, not a hardcoded
//! shortcut" pattern (e.g. `autocmd.rs`'s `apply_autocmds` bypass
//! path). The `curtab.tp_diff_invalid` check (which would call the
//! substantial, untranslated `ex_diffupdate`) is ALSO always false
//! today (nothing sets it), so it's checked for real too rather than
//! assumed away; and `diff_buf_idx`/[`diff_mode_buf`] - `diff_buf_idx`
//! is a plain linear scan through `TabpageT.tp_diffbuf[]` (already a
//! real field), and `diff_mode_buf` walks every tabpage via
//! `GLOBALS.first_tabpage`/`tp_next` (the same walk already
//! established by `window.rs`'s `win_valid_any_tab`) - genuinely
//! COMPLETE translations, not fast-path-only, since nothing about
//! either depends on a real diff actually existing.
//!
//! Also translated: [`diff_get_corresponding_line`]/[`diff_lnum_win`]/
//! [`diff_move_to`] (plus the private `diff_get_corresponding_line_int`) -
//! real, faithful translations of their "current buffer isn't a diff
//! buffer" early-return path (always taken today since `diff_buf_idx`
//! always returns `DB_COUNT`, matching the same reasoning as
//! `diff_check_with_linestatus`), translated ahead of any real caller
//! (none of `winfloat.c`/`move.c`/`window.c`'s own diff-aware
//! scroll-binding callers are translated yet).
//!
//! Also translated: [`diff_update_line`] - notably, its OWN first
//! early return (`!(diff_flags & ALL_INLINE_DIFF)`) is genuinely NOT
//! always taken today (the real `'diffopt'` default includes
//! `inline:char`) - translated for real rather than assumed; it's the
//! SECOND check (`diff_buf_idx` returning `DB_COUNT`) that is always
//! taken today, for the same reason as the functions above.
//!
//! Also translated: [`diff_infold`] - its own `idx`/`other`-computing
//! loop over `tp_diffbuf[]` is real, complete logic (not stubbed),
//! since it only reads already-real fields; only its OWN early-return
//! condition (`idx == -1 || !other`) happens to always be taken today,
//! for the same underlying reason as the functions above.
//!
//! Also translated: [`diff_mark_adjust`] - `mark_adjust_buf`'s
//! (`mark.c`) own third real dependency (alongside `quickfix.c`'s
//! `qf_mark_adjust` and `fold.c`'s `foldMarkAdjust`). Walks tab pages
//! the same way [`diff_mode_buf`] already does; its own real
//! per-tabpage adjustment (`diff_mark_adjust_tp`) is genuinely,
//! provably unreachable today (same `diff_buf_idx` reasoning as
//! everything else in this file) and is not translated at all.
//!
//! Also translated: [`diff_equal_char`] (compares 2 characters,
//! honoring `'diffopt'`'s `icase` flag - a genuinely self-contained
//! leaf function, needing only already-real `crate::mbyte::
//! utfc_ptr2len`/`utf_fold`/`utf_ptr2char` and `crate::macros_defs::
//! tolower_loc`) - translated ahead of its own real caller
//! (`diff_equal_entry`, needing `curtab.tp_diffbuf`/`ml_get_buf`/
//! `diff_check_sanity`, not translated).
//!
//! Deferred: everything else in the file - real diff computation/
//! display/navigation, needing the internal xdiff algorithm or
//! external `diff` process invocation, neither translated.

use crate::buffer_defs::WinT;

/// `DIFF_*` flags for [`DIFF_FLAGS`] (`diff_flags`' own bit values).
pub mod diff_flag {
    /// display filler lines (`DIFF_FILLER`).
    pub const FILLER: i32 = 0x001;
    /// ignore empty lines (`DIFF_IBLANK`).
    pub const IBLANK: i32 = 0x002;
    /// ignore case (`DIFF_ICASE`).
    pub const ICASE: i32 = 0x004;
    /// ignore change in white space (`DIFF_IWHITE`).
    pub const IWHITE: i32 = 0x008;
    /// ignore all white space changes (`DIFF_IWHITEALL`).
    pub const IWHITEALL: i32 = 0x010;
    /// ignore change in white space at EOL (`DIFF_IWHITEEOL`).
    pub const IWHITEEOL: i32 = 0x020;
    /// horizontal splits (`DIFF_HORIZONTAL`).
    pub const HORIZONTAL: i32 = 0x040;
    /// vertical splits (`DIFF_VERTICAL`).
    pub const VERTICAL: i32 = 0x080;
    /// diffoff when hidden (`DIFF_HIDDEN_OFF`).
    pub const HIDDEN_OFF: i32 = 0x100;
    /// use internal xdiff algorithm (`DIFF_INTERNAL`).
    pub const INTERNAL: i32 = 0x200;
    /// diffoff when closing window (`DIFF_CLOSE_OFF`).
    pub const CLOSE_OFF: i32 = 0x400;
    /// follow the wrap option (`DIFF_FOLLOWWRAP`).
    pub const FOLLOWWRAP: i32 = 0x800;
    /// match most similar lines within diff (`DIFF_LINEMATCH`).
    pub const LINEMATCH: i32 = 0x1000;
    /// no inline highlight (`DIFF_INLINE_NONE`).
    pub const INLINE_NONE: i32 = 0x2000;
    /// inline highlight with simple algorithm (`DIFF_INLINE_SIMPLE`).
    pub const INLINE_SIMPLE: i32 = 0x4000;
    /// inline highlight with character diff (`DIFF_INLINE_CHAR`).
    pub const INLINE_CHAR: i32 = 0x8000;
    /// inline highlight with word diff (`DIFF_INLINE_WORD`).
    pub const INLINE_WORD: i32 = 0x10000;
    /// use `'diffanchors'` to anchor the diff (`DIFF_ANCHOR`).
    pub const ANCHOR: i32 = 0x20000;
}

/// Combination of both inline-diff-caching flags (`ALL_INLINE_DIFF`).
pub const ALL_INLINE_DIFF: i32 = diff_flag::INLINE_CHAR | diff_flag::INLINE_WORD;

/// `diff_flags` - the parsed bit-flag form of `'diffopt'`. A file-
/// static in the original; translated as a `pub` `GlobalCell` since
/// (unlike most of this crate's file-statics) a real, currently-
/// reachable caller (this module's own [`diffopt_filler`]/
/// [`diffopt_closeoff`]) reads it. Initialized to the EXACT value the
/// original's own static initializer uses (see this module's own doc
/// comment) - not zero, since `'diffopt'`'s real default is NOT empty.
pub static DIFF_FLAGS: crate::globals::GlobalCell<i32> = crate::globals::GlobalCell::new(
    diff_flag::INTERNAL | diff_flag::FILLER | diff_flag::CLOSE_OFF | diff_flag::LINEMATCH
        | diff_flag::INLINE_CHAR,
);

/// Set when `diff_redraw()` still needs to be called
/// (`need_diff_redraw`).
///
/// While this is set, fold updates are postponed, since the diff
/// itself is about to be recomputed anyway.
pub static NEED_DIFF_REDRAW: crate::globals::GlobalCell<bool> =
    crate::globals::GlobalCell::new(false);

/// Free one diff block (`clear_diffblock`).
///
/// # Safety
/// `dp` must be a non-null pointer to a `DiffT` that was allocated as
/// a `Box` and is not referenced anywhere else; it is invalid
/// afterwards.
pub unsafe fn clear_diffblock(dp: *mut crate::buffer_defs::DiffT) {
    if dp.is_null() {
        return;
    }
    // SAFETY: forwarded from this function's own safety doc. Taking
    // the Box back subsumes the original's `ga_clear(&dp->df_changes)`
    // plus `xfree(dp)`: dropping it frees the growable array too.
    drop(unsafe { Box::from_raw(dp) });
}

/// Allocate a new diff block and link it into `tp`'s chain between
/// `dprev` and `dp` (`diff_alloc_new`).
///
/// `dprev` being null means the new block becomes the head of the
/// chain. The returned pointer is a leaked `Box`, matching this
/// module's own established ownership convention - `diff_clear`/
/// `clear_diffblock` take it back.
///
/// The original's `ga_init(&dnew->df_changes, ..., 20)` sets an
/// initial growth step on the array; this crate's `GarrayT` grows
/// through `Vec`, which needs no such tuning, so a default array is
/// used instead.
///
/// # Safety
/// `dprev`, when non-null, must be a valid pointer to a live `DiffT`
/// belonging to `tp`'s chain.
pub unsafe fn diff_alloc_new(
    tp: &mut crate::buffer_defs::TabpageT,
    dprev: *mut crate::buffer_defs::DiffT,
    dp: *mut crate::buffer_defs::DiffT,
) -> *mut crate::buffer_defs::DiffT {
    let dnew = Box::into_raw(Box::new(crate::buffer_defs::DiffT {
        is_linematched: false,
        df_next: dp,
        has_changes: false,
        df_changes: crate::garray_defs::GarrayT::default(),
        ..Default::default()
    }));

    if dprev.is_null() {
        tp.tp_first_diff = dnew;
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*dprev).df_next = dnew };
    }

    dnew
}

/// Remove `buf` from every tab page's list of diff buffers
/// (`diff_buf_delete`).
///
/// Each tab page that held the buffer has its diff list marked
/// outdated. For the CURRENT tab page a redraw is also requested, but
/// deliberately deferred (`need_diff_redraw`) rather than done
/// immediately: more may still change, and the buffer state is
/// invalid right now.
///
/// As elsewhere in this crate, the original's `FOR_ALL_TABS(tp)` is
/// walked as `GLOBALS.first_tabpage`/`tp_next`.
///
/// # Safety
/// `GLOBALS.first_tabpage`'s own `tp_next` chain must consist of
/// valid, live `TabpageT` pointers, and `GLOBALS.curwin` must be
/// valid when the current tab page is reached.
pub unsafe fn diff_buf_delete(buf: *mut crate::buffer_defs::BufT) {
    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    let curtab = g.curtab;
    let curwin = g.curwin;
    let mut tp = g.first_tabpage;

    while !tp.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        let next = unsafe { (*tp).tp_next };
        let i = diff_buf_idx(buf, tp);
        if i != crate::buffer_defs::DB_COUNT {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe {
                (*tp).tp_diffbuf[i] = std::ptr::null_mut();
                (*tp).tp_diff_invalid = 1;
            };

            if std::ptr::eq(tp, curtab) {
                // Don't redraw right away, more might change or the
                // buffer state is invalid right now.
                // SAFETY: plain GlobalCell write.
                unsafe { *NEED_DIFF_REDRAW.get_mut() = true };
                // SAFETY: forwarded from this function's own safety doc.
                unsafe {
                    crate::drawscreen::redraw_later(curwin, crate::drawscreen::UPD_VALID);
                };
            }
        }
        tp = next;
    }
}

/// Unlink and free diff block `dp` from `tp`'s chain, returning the
/// block that followed it (`diff_free`).
///
/// `dprev` being null means `dp` was the head, so its successor takes
/// over. The returned pointer lets a caller keep walking the chain
/// after the removal.
///
/// The successor is read BEFORE `dp` is freed, since reading it
/// afterwards would be a use-after-free - the same ordering
/// [`diff_clear`] relies on.
///
/// # Safety
/// `dp` must be a valid, non-null pointer to a live `Box`-allocated
/// `DiffT` in `tp`'s chain, not referenced elsewhere. `dprev`, when
/// non-null, must likewise be live and be `dp`'s predecessor.
pub unsafe fn diff_free(
    tp: &mut crate::buffer_defs::TabpageT,
    dprev: *mut crate::buffer_defs::DiffT,
    dp: *mut crate::buffer_defs::DiffT,
) -> *mut crate::buffer_defs::DiffT {
    // SAFETY: forwarded from this function's own safety doc - read
    // the successor before freeing.
    let ret = unsafe { (*dp).df_next };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { clear_diffblock(dp) };

    if dprev.is_null() {
        tp.tp_first_diff = ret;
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*dprev).df_next = ret };
    }

    ret
}

/// Input to a diff operation (`diffin_T`).
///
/// Exactly one of the two fields is in use: an external diff writes
/// the text to a temporary FILE and names it in `din_fname`, while an
/// internal diff keeps it in memory. `din_fname` being unset is what
/// distinguishes the two, and drives [`clear_diffin`].
///
/// The original's `mmfile_t` (an xdiff `{ptr, size}` pair) becomes an
/// owned `Vec<u8>`, matching `linematch.rs`'s own treatment of the
/// same type.
#[derive(Debug, Default)]
pub struct DiffinT {
    /// Temporary file holding the text, for an external diff
    /// (`din_fname`).
    pub din_fname: Option<Vec<u8>>,
    /// The text itself, for an internal diff (`din_mmfile`).
    pub din_mmfile: Vec<u8>,
}

/// One hunk produced by the diff engine (`diffhunk_T`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DiffhunkT {
    /// First original-file line in the hunk (`lnum_orig`).
    pub lnum_orig: crate::pos_defs::LinenrT,
    /// Number of original-file lines (`count_orig`).
    pub count_orig: i32,
    /// First new-file line in the hunk (`lnum_new`).
    pub lnum_new: crate::pos_defs::LinenrT,
    /// Number of new-file lines (`count_new`).
    pub count_new: i32,
}

/// Result of a diff operation (`diffout_T`).
///
/// Mirrors [`DiffinT`]: an external diff leaves its output in a
/// temporary file, an internal one in a growable array.
#[derive(Debug)]
pub struct DiffoutT {
    /// Temporary file holding the result, for an external diff
    /// (`dout_fname`).
    pub dout_fname: Option<Vec<u8>>,
    /// The result itself, for an internal diff (`dout_ga`).
    pub dout_ga: crate::garray_defs::TypedGarrayT<DiffhunkT>,
}

impl Default for DiffoutT {
    fn default() -> Self {
        Self {
            dout_fname: None,
            dout_ga: crate::garray_defs::TypedGarrayT::new(100),
        }
    }
}

/// Release whichever half of `din` is actually in use
/// (`clear_diffin`).
///
/// With no temporary file the in-memory buffer is released; with one,
/// the FILE is deleted instead. Getting that branch backwards would
/// leak a temp file on every external diff, so it is what the tests
/// check.
pub fn clear_diffin(din: &mut DiffinT) {
    match din.din_fname.as_ref() {
        None => din.din_mmfile.clear(),
        Some(fname) => {
            if let Ok(s) = std::str::from_utf8(fname) {
                crate::os::fs::os_remove(std::path::Path::new(s));
            }
        }
    }
}

/// Release whichever half of `dout` is actually in use
/// (`clear_diffout`).
pub fn clear_diffout(dout: &mut DiffoutT) {
    match dout.dout_fname.as_ref() {
        None => dout.dout_ga.ga_clear(),
        Some(fname) => {
            if let Ok(s) = std::str::from_utf8(fname) {
                crate::os::fs::os_remove(std::path::Path::new(s));
            }
        }
    }
}

/// Copies the next in-memory diff hunk into `hunk`
/// (`extract_hunk_internal`).
///
/// Returns `true` at end of input. On EOF both `hunk` and `line_idx`
/// are left untouched.
pub fn extract_hunk_internal(
    dout: &DiffoutT,
    hunk: &mut DiffhunkT,
    line_idx: &mut usize,
) -> bool {
    if *line_idx >= dout.dout_ga.items.len() {
        return true;
    }
    *hunk = dout.dout_ga.items[*line_idx];
    *line_idx += 1;
    false
}

/// Records one internal-xdiff result hunk (`xdiff_out`).
///
/// xdiff reports zero-based start lines; Neovim stores one-based line
/// numbers in `DiffhunkT`. The callback always returns zero.
pub fn xdiff_out(
    start_a: i32,
    count_a: i32,
    start_b: i32,
    count_b: i32,
    dout: &mut DiffoutT,
) -> i32 {
    dout.dout_ga.items.push(DiffhunkT {
        lnum_orig: start_a + 1,
        count_orig: count_a,
        lnum_new: start_b + 1,
        count_new: count_b,
    });
    0
}

/// Parses one ed-style diff command into `hunk` (`parse_diff_ed`).
///
/// Accepted forms are `{first}[,{last}]c...`, `...a...`, and
/// `...d...`. Trailing text after the second range is ignored, as in
/// the original parser.
pub fn parse_diff_ed(line: &[u8], hunk: &mut DiffhunkT) -> i32 {
    let mut p = 0;
    let (f1, used) = crate::charset::getdigits_int32(&line[p..], true, 0);
    p += used;
    let l1 = if line.get(p) == Some(&b',') {
        p += 1;
        let (value, used) = crate::charset::getdigits_int(&line[p..], true, 0);
        p += used;
        value
    } else {
        f1
    };

    let Some(&difftype @ (b'a' | b'c' | b'd')) = line.get(p) else {
        return crate::vim_defs::FAIL;
    };
    p += 1;

    let (f2, used) = crate::charset::getdigits_int(&line[p..], true, 0);
    p += used;
    let l2 = if line.get(p) == Some(&b',') {
        p += 1;
        let (value, _used) = crate::charset::getdigits_int(&line[p..], true, 0);
        value
    } else {
        f2
    };

    if l1 < f1 || l2 < f2 {
        return crate::vim_defs::FAIL;
    }

    if difftype == b'a' {
        hunk.lnum_orig = f1 + 1;
        hunk.count_orig = 0;
    } else {
        hunk.lnum_orig = f1;
        hunk.count_orig = l1 - f1 + 1;
    }
    if difftype == b'd' {
        hunk.lnum_new = f2 + 1;
        hunk.count_new = 0;
    } else {
        hunk.lnum_new = f2;
        hunk.count_new = l2 - f2 + 1;
    }
    crate::vim_defs::OK
}

/// Copy one diff entry's line range from one buffer slot to another
/// (`diff_copy_entry`).
///
/// `dprev` is the entry before `dp`, or `None` for the first one. The
/// line-number offset that everything above `dp` has accumulated
/// between the two slots is subtracted, so `idx_new` describes the
/// same change `idx_orig` does, expressed in the other buffer's own
/// line numbers.
pub fn diff_copy_entry(
    dprev: Option<&crate::buffer_defs::DiffT>,
    dp: &mut crate::buffer_defs::DiffT,
    idx_orig: usize,
    idx_new: usize,
) {
    let off = match dprev {
        None => 0,
        Some(prev) => {
            (prev.df_lnum[idx_orig] + prev.df_count[idx_orig])
                - (prev.df_lnum[idx_new] + prev.df_count[idx_new])
        }
    };
    dp.df_lnum[idx_new] = dp.df_lnum[idx_orig] - off;
    dp.df_count[idx_new] = dp.df_count[idx_orig];
}

/// Whether every diff block line range in `dp` fits inside its own
/// buffer (`diff_check_sanity`).
///
/// Returns `FAIL` for the first buffer whose range runs past the end,
/// `OK` when every registered buffer is consistent. Slots with no
/// registered buffer are skipped.
///
/// # Safety
/// Every non-null entry in `tp.tp_diffbuf` must be a valid pointer to
/// a live `BufT`.
#[must_use]
pub unsafe fn diff_check_sanity(
    tp: &crate::buffer_defs::TabpageT,
    dp: &crate::buffer_defs::DiffT,
) -> i32 {
    for i in 0..crate::buffer_defs::DB_COUNT {
        let buf = tp.tp_diffbuf[i];
        if buf.is_null() {
            continue;
        }
        // SAFETY: forwarded from this function's own safety doc.
        let line_count = unsafe { (*buf).b_ml.ml_line_count };
        if dp.df_lnum[i] + dp.df_count[i] - 1 > line_count {
            return crate::vim_defs::FAIL;
        }
    }
    crate::vim_defs::OK
}

/// Free the whole list of diff blocks for tab page `tp`
/// (`diff_clear`).
///
/// Each `df_next` is read BEFORE its block is freed, since reading it
/// afterwards would be a use-after-free.
///
/// # Safety
/// `tp.tp_first_diff` must be either null or a valid chain of
/// `Box`-allocated `DiffT`s, none referenced elsewhere.
pub unsafe fn diff_clear(tp: &mut crate::buffer_defs::TabpageT) {
    let mut p = tp.tp_first_diff;
    while !p.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        let next = unsafe { (*p).df_next };
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { clear_diffblock(p) };
        p = next;
    }
    tp.tp_first_diff = std::ptr::null_mut();
}

/// Compare two line numbers (`lnum_compare`), the original's `qsort`
/// comparator for sorting a list of line numbers.
///
/// Returns [`std::cmp::Ordering`] rather than a C comparator's
/// negative/zero/positive `int`, so it drops straight into Rust's own
/// `sort_by` - the shape already used for `fuzzy.rs`'s comparators.
#[must_use]
pub fn lnum_compare(
    lnum1: crate::pos_defs::LinenrT,
    lnum2: crate::pos_defs::LinenrT,
) -> std::cmp::Ordering {
    lnum1.cmp(&lnum2)
}

/// Return `true` if `'diffopt'` contains `"closeoff"` (`diffopt_closeoff`).
#[must_use]
pub fn diffopt_closeoff() -> bool {
    (unsafe { *DIFF_FLAGS.get_mut() }) & diff_flag::CLOSE_OFF != 0
}

/// Return `true` if `'diffopt'` contains `"filler"` (`diffopt_filler`).
#[must_use]
pub fn diffopt_filler() -> bool {
    (unsafe { *DIFF_FLAGS.get_mut() }) & diff_flag::FILLER != 0
}

/// Return `true` when the internal xdiff algorithm should be used
/// rather than an external `diff` command (`diff_internal`).
///
/// That is the case when `'diffopt'` contains `"internal"` and
/// `'diffexpr'` is empty.
#[must_use]
pub fn diff_internal() -> bool {
    (unsafe { *DIFF_FLAGS.get_mut() }) & diff_flag::INTERNAL != 0
        && unsafe { crate::option_vars::OPTION_VARS.get_mut() }
            .p_dex
            .as_deref()
            .is_none_or(<[u8]>::is_empty)
}

/// Return `true` if `'diffopt'` contains `"horizontal"`
/// (`diffopt_horizontal`).
#[must_use]
pub fn diffopt_horizontal() -> bool {
    (unsafe { *DIFF_FLAGS.get_mut() }) & diff_flag::HORIZONTAL != 0
}

/// Return `true` if `'diffopt'` contains `"hiddenoff"`
/// (`diffopt_hiddenoff`).
#[must_use]
pub fn diffopt_hiddenoff() -> bool {
    (unsafe { *DIFF_FLAGS.get_mut() }) & diff_flag::HIDDEN_OFF != 0
}

/// Compare the characters at `p1` and `p2`. If they are equal
/// (possibly ignoring case, per `'diffopt'`'s `icase` flag), returns
/// the number of bytes they occupy; otherwise `None`
/// (`diff_equal_char`).
///
/// Deviates from the original's `int *const len` out-parameter
/// (always uninitialized on a `false`/non-match return) by folding
/// the byte length into the `Some`/`None` result directly.
///
/// # Safety
/// `p1`/`p2` must be non-empty and point to valid, well-formed UTF-8
/// byte sequences (forwarded from `crate::mbyte::utfc_ptr2len`'s own
/// safety contract).
#[must_use]
pub unsafe fn diff_equal_char(p1: &[u8], p2: &[u8]) -> Option<usize> {
    // SAFETY: forwarded from this function's own safety doc.
    let l = usize::try_from(unsafe { crate::mbyte::utfc_ptr2len(p1) }).unwrap_or(0);
    // SAFETY: forwarded from this function's own safety doc.
    if l != usize::try_from(unsafe { crate::mbyte::utfc_ptr2len(p2) }).unwrap_or(0) {
        return None;
    }
    let icase = (unsafe { *DIFF_FLAGS.get_mut() }) & diff_flag::ICASE != 0;
    if l > 1 {
        if p1[..l] != p2[..l]
            && (!icase
                || crate::mbyte::utf_fold(crate::mbyte::utf_ptr2char(p1))
                    != crate::mbyte::utf_fold(crate::mbyte::utf_ptr2char(p2)))
        {
            return None;
        }
        Some(l)
    } else {
        if p1[0] != p2[0]
            && (!icase
                || crate::macros_defs::tolower_loc(i32::from(p1[0]))
                    != crate::macros_defs::tolower_loc(i32::from(p2[0])))
        {
            return None;
        }
        Some(1)
    }
}

/// Return the diff status of `lnum` in window `wp`'s buffer,
/// optionally reporting a line-status code via `linestatus`
/// (`diff_check_with_linestatus`). This should only be used for
/// windows where `'diff'` is set.
///
/// Only the "no diffs at all in this tab page" early-return path is
/// translated (see this module's own doc comment) - the real diff-
/// hunk search (the `tp_first_diff` linked-list walk, now using the
/// already-real `diff_buf_idx`) is `unimplemented!()`, unreachable
/// in practice today since nothing in this crate can create a diff.
/// `lnum` is accepted for signature fidelity (the real function's own
/// later "lnum must be a buffer line" safety check, and the diff-hunk
/// search itself, both need it) but genuinely unused by the
/// early-return path translated here.
///
/// # Safety
/// `crate::globals::GLOBALS.curtab` must be a valid, non-null pointer
/// to a live `TabpageT`.
#[must_use]
pub unsafe fn diff_check_with_linestatus(
    wp: &WinT,
    _lnum: crate::pos_defs::LinenrT,
    linestatus: Option<&mut i32>,
) -> i32 {
    if let Some(ls) = linestatus {
        *ls = 0;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let curtab = unsafe { &*crate::globals::GLOBALS.get_mut().curtab };

    if curtab.tp_diff_invalid != 0 {
        // update after a big change - needs the real, substantial
        // ex_diffupdate, not yet translated. Unreachable in practice
        // today: nothing in this crate can currently set
        // tp_diff_invalid to a nonzero value.
        unimplemented!(
            "diff::diff_check_with_linestatus: ex_diffupdate is not yet translated - \
             unreachable in practice today since tp_diff_invalid is always 0"
        );
    }

    // no diffs at all
    if curtab.tp_first_diff.is_null() || wp.w_onebuf_opt.wo_diff == 0 {
        return 0;
    }

    unimplemented!(
        "diff::diff_check_with_linestatus: the real diff-hunk search is not yet translated - \
         unreachable in practice today since tp_first_diff is always null, see this module's \
         own doc comment"
    );
}

/// See [`diff_check_with_linestatus`] (`diff_check_fill`).
///
/// # Safety
/// Same as [`diff_check_with_linestatus`].
#[must_use]
pub unsafe fn diff_check_fill(wp: &WinT, lnum: crate::pos_defs::LinenrT) -> i32 {
    // be quick when there are no filler lines
    if !diffopt_filler() {
        return 0;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let n = unsafe { diff_check_with_linestatus(wp, lnum, None) };
    n.max(0)
}

/// Return the index of `buf` in `tp`'s `tp_diffbuf[]` array, or
/// [`crate::buffer_defs::DB_COUNT`] if `buf` isn't currently
/// registered there (`diff_buf_idx`).
///
/// # Safety
/// `tp` must be a valid, non-null pointer to a live
/// [`crate::buffer_defs::TabpageT`].
fn diff_buf_idx(buf: *mut crate::buffer_defs::BufT, tp: *mut crate::buffer_defs::TabpageT) -> usize {
    // SAFETY: forwarded from this function's own safety doc.
    let tp = unsafe { &*tp };
    tp.tp_diffbuf
        .iter()
        .position(|&b| b == buf)
        .unwrap_or(crate::buffer_defs::DB_COUNT)
}

/// Return `true` if `diff` appears in the current tab page's list of
/// diff blocks (`valid_diff`). Its only real caller (deep inside the
/// substantial, untranslated `ex_diffupdate`/fold-update machinery)
/// isn't translated yet - harvested ahead of it, matching this
/// crate's established precedent for a small, self-contained function
/// with no design freedom of its own.
///
/// `GLOBALS.curtab.tp_first_diff` is always null today (nothing in
/// this crate can currently create a diff block, see this module's
/// own doc comment), so this always returns `false` in practice - a
/// genuinely correct, total answer for every `diff` pointer (no diff
/// blocks exist, so no pointer is ever a member of that empty list),
/// unlike e.g. `normal.rs`'s `op_pending` (where an empty/uninitialized
/// registry does NOT mean the same thing as "genuinely not pending").
///
/// # Safety
/// `crate::globals::GLOBALS.curtab` must be a valid, non-null pointer
/// to a live [`crate::buffer_defs::TabpageT`], and every diff block
/// transitively reachable through `df_next` from its `tp_first_diff`
/// must likewise be valid.
#[must_use]
pub unsafe fn valid_diff(diff: *const crate::buffer_defs::DiffT) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let curtab = unsafe { &*crate::globals::GLOBALS.get_mut().curtab };
    let mut dp = curtab.tp_first_diff;
    while !dp.is_null() {
        if std::ptr::eq(dp, diff) {
            return true;
        }
        // SAFETY: forwarded from this function's own safety doc.
        dp = unsafe { &*dp }.df_next;
    }
    false
}

/// Return `true` if `buf` is being diffed in any tab page
/// (`diff_mode_buf`).
///
/// # Safety
/// `crate::globals::GLOBALS.first_tabpage` must be null or a valid
/// pointer to a live [`crate::buffer_defs::TabpageT`], and every
/// tabpage transitively reachable through `tp_next` from there must
/// likewise be valid.
#[must_use]
pub unsafe fn diff_mode_buf(buf: *mut crate::buffer_defs::BufT) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let mut tp = unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage;
    while !tp.is_null() {
        if diff_buf_idx(buf, tp) != crate::buffer_defs::DB_COUNT {
            return true;
        }
        // SAFETY: forwarded from this function's own safety doc.
        tp = unsafe { &*tp }.tp_next;
    }
    false
}

/// Adjust diffs in every tab page that has `buf` registered as one of
/// its diff buffers, for a change in line numbers (`diff_mark_adjust`).
///
/// Walks tab pages the same way [`diff_mode_buf`] already does. The
/// original's own real per-tabpage adjustment (`diff_mark_adjust_tp`)
/// is called only when `diff_buf_idx(buf, tp) != DB_COUNT` - since
/// that never happens today (see this module's own doc comment,
/// nothing can register a buffer into any `tp_diffbuf[]`), this
/// function's loop body never actually runs, and
/// `diff_mark_adjust_tp` itself is not translated at all (not even as
/// an `unimplemented!()` stub) - genuinely, provably unreachable
/// today, matching `qf_mark_adjust`'s own established precedent for
/// this exact situation.
///
/// # Safety
/// Same as [`diff_mode_buf`].
pub unsafe fn diff_mark_adjust(
    buf: *mut crate::buffer_defs::BufT,
    _line1: crate::pos_defs::LinenrT,
    _line2: crate::pos_defs::LinenrT,
    _amount: crate::pos_defs::LinenrT,
    _amount_after: crate::pos_defs::LinenrT,
) {
    // SAFETY: forwarded from this function's own safety doc.
    let mut tp = unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage;
    while !tp.is_null() {
        if diff_buf_idx(buf, tp) != crate::buffer_defs::DB_COUNT {
            unreachable!(
                "diff_mark_adjust: diff_buf_idx never returns anything but DB_COUNT today, see \
                 this function's own doc comment"
            );
        }
        // SAFETY: forwarded from this function's own safety doc.
        tp = unsafe { &*tp }.tp_next;
    }
}

/// Find the corresponding line in a diff (`diff_get_corresponding_line_int`).
///
/// Only the "no diffs at all" early-return path is translated (see
/// this module's own doc comment) - the real diff-block search
/// (walking `tp_first_diff`) is `unimplemented!()`, unreachable in
/// practice today since `diff_buf_idx` always returns `DB_COUNT`
/// (nothing in this crate can currently register a buffer as a diff
/// buffer).
///
/// # Safety
/// `GLOBALS.curbuf`/`curwin`/`curtab` must each be a valid, non-null
/// pointer to a live value.
unsafe fn diff_get_corresponding_line_int(
    buf1: *mut crate::buffer_defs::BufT,
    lnum1: crate::pos_defs::LinenrT,
) -> crate::pos_defs::LinenrT {
    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    let idx1 = diff_buf_idx(buf1, g.curtab);
    let idx2 = diff_buf_idx(g.curbuf, g.curtab);

    // SAFETY: forwarded from this function's own safety doc.
    let tp_first_diff_is_null = unsafe { &*g.curtab }.tp_first_diff.is_null();

    if idx1 == crate::buffer_defs::DB_COUNT
        || idx2 == crate::buffer_defs::DB_COUNT
        || tp_first_diff_is_null
    {
        return lnum1;
    }

    unimplemented!(
        "diff::diff_get_corresponding_line_int: the real diff-block search is not yet \
         translated - unreachable in practice today since diff_buf_idx always returns \
         DB_COUNT, see this module's own doc comment"
    );
}

/// Find the corresponding line in a diff, clamped so it never lands
/// past the end of the current buffer (`diff_get_corresponding_line`).
/// Translated ahead of a real caller (none of `winfloat.c`/
/// `move.c`/`window.c`'s own diff-aware scroll-binding callers are
/// translated yet), matching this crate's established "small,
/// self-contained piece ahead of the surrounding engine" precedent.
///
/// # Safety
/// Same as `diff_get_corresponding_line_int`.
#[must_use]
pub unsafe fn diff_get_corresponding_line(
    buf1: *mut crate::buffer_defs::BufT,
    lnum1: crate::pos_defs::LinenrT,
) -> crate::pos_defs::LinenrT {
    // SAFETY: forwarded from this function's own safety doc.
    let lnum = unsafe { diff_get_corresponding_line_int(buf1, lnum1) };
    // don't end up past the end of the file
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &*crate::globals::GLOBALS.get_mut().curbuf };
    lnum.min(curbuf.b_ml.ml_line_count)
}

/// For line `lnum` in the current window, find the equivalent line
/// number in window `wp`, compensating for inserted/deleted lines
/// (`diff_lnum_win`).
///
/// Only the "current buffer isn't a diff buffer" safety-check
/// early-return is translated - always taken today since
/// `diff_buf_idx` always returns `DB_COUNT` (see this module's own
/// doc comment). The real diff-block search is `unimplemented!()`.
///
/// # Safety
/// `GLOBALS.curbuf`/`curtab` must each be a valid, non-null pointer to
/// a live value.
#[must_use]
pub unsafe fn diff_lnum_win(
    _lnum: crate::pos_defs::LinenrT,
    _wp: *mut WinT,
) -> crate::pos_defs::LinenrT {
    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    let idx = diff_buf_idx(g.curbuf, g.curtab);

    if idx == crate::buffer_defs::DB_COUNT {
        // safety check
        return 0;
    }

    unimplemented!(
        "diff::diff_lnum_win: the real diff-block search is not yet translated - unreachable \
         in practice today since diff_buf_idx always returns DB_COUNT, see this module's own \
         doc comment"
    );
}

/// Move `count` times in direction `dir` to the next diff block
/// (`diff_move_to`).
///
/// Only the "current buffer isn't a diff buffer, or there are no
/// diffs at all" early-return path is translated - always taken today
/// since `diff_buf_idx` always returns `DB_COUNT` (see this module's
/// own doc comment). The real diff-block search/cursor-move logic
/// beyond that point is `unimplemented!()`.
///
/// # Safety
/// `GLOBALS.curbuf`/`curwin`/`curtab` must each be a valid, non-null
/// pointer to a live value.
#[must_use]
pub unsafe fn diff_move_to(_dir: i32, _count: i32) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    let idx = diff_buf_idx(g.curbuf, g.curtab);
    // SAFETY: forwarded from this function's own safety doc.
    let tp_first_diff_is_null = unsafe { &*g.curtab }.tp_first_diff.is_null();

    if idx == crate::buffer_defs::DB_COUNT || tp_first_diff_is_null {
        return crate::vim_defs::FAIL;
    }

    unimplemented!(
        "diff::diff_move_to: the real diff-block search/cursor-move logic is not yet \
         translated - unreachable in practice today since diff_buf_idx always returns \
         DB_COUNT, see this module's own doc comment"
    );
}

/// Called when a line has been updated - clears the cached inline
/// diff for the diff block containing it, if any, so it is recomputed
/// (`diff_update_line`).
///
/// Unlike this file's other `diff_buf_idx`-gated functions, the FIRST
/// early return here (`!(diff_flags & ALL_INLINE_DIFF)`) is genuinely
/// NOT always taken today: the real `'diffopt'` default includes
/// `inline:char`, setting `DIFF_INLINE_CHAR` - so this crate's own
/// `DIFF_FLAGS` default already has a bit of `ALL_INLINE_DIFF` set,
/// and this check is translated for real rather than assumed always
/// true. It is the SECOND check (`diff_buf_idx` returning `DB_COUNT`)
/// that is always taken today, for the same reason as this file's
/// other `diff_buf_idx`-gated functions. The real diff-block search
/// beyond both checks is `unimplemented!()`.
///
/// # Safety
/// `GLOBALS.curbuf`/`curtab` must each be a valid, non-null pointer to
/// a live value.
pub unsafe fn diff_update_line(_lnum: crate::pos_defs::LinenrT) {
    // We only care if we are doing inline-diff where we cache the diff results
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { *DIFF_FLAGS.get_mut() } & ALL_INLINE_DIFF == 0 {
        return;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    let idx = diff_buf_idx(g.curbuf, g.curtab);
    if idx == crate::buffer_defs::DB_COUNT {
        return;
    }

    unimplemented!(
        "diff::diff_update_line: the real diff-block search is not yet translated - \
         unreachable in practice today since diff_buf_idx always returns DB_COUNT, see this \
         module's own doc comment"
    );
}

/// Return `true` if `lnum` in window `wp` is hidden by folding due to
/// a closed diff (`diff_infold`).
///
/// The "does this window's own buffer appear in `tp_diffbuf[]`, and is
/// there at least one OTHER real diff buffer too" loop is translated
/// in full (not stubbed) - it's genuine, self-contained logic over
/// already-real fields, faithfully correct for any future test that
/// manually populates `tp_diffbuf`. Its own early return (`idx == -1 ||
/// !other`) is always taken today (nothing in this crate can currently
/// register a buffer in `tp_diffbuf`, so `idx` always stays `-1`). The
/// real diff-block search beyond that point is `unimplemented!()`.
///
/// # Safety
/// `crate::globals::GLOBALS.curtab` must be a valid, non-null pointer
/// to a live [`crate::buffer_defs::TabpageT`].
#[must_use]
pub unsafe fn diff_infold(wp: &WinT, _lnum: crate::pos_defs::LinenrT) -> bool {
    // Return if 'diff' isn't set.
    if wp.w_onebuf_opt.wo_diff == 0 {
        return false;
    }

    let mut idx: i32 = -1;
    let mut other = false;
    // SAFETY: forwarded from this function's own safety doc.
    let curtab = unsafe { &*crate::globals::GLOBALS.get_mut().curtab };
    for (i, &b) in curtab.tp_diffbuf.iter().enumerate() {
        if b == wp.w_buffer {
            idx = i32::try_from(i).expect("DB_COUNT is small, always fits in an i32");
        } else if !b.is_null() {
            other = true;
        }
    }

    // return here if there are no diffs in the window
    if idx == -1 || !other {
        return false;
    }

    unimplemented!(
        "diff::diff_infold: the real diff-block search is not yet translated - unreachable in \
         practice today since idx==-1 is always true (nothing can register a buffer in \
         tp_diffbuf), see this module's own doc comment"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer_defs::BufT;

    #[test]
    fn diffhunk_holds_both_original_and_new_ranges_independently() {
        let hunk = DiffhunkT {
            lnum_orig: 3,
            count_orig: 4,
            lnum_new: 8,
            count_new: 2,
        };
        assert_eq!((hunk.lnum_orig, hunk.count_orig), (3, 4));
        assert_eq!((hunk.lnum_new, hunk.count_new), (8, 2));
    }

    #[test]
    fn diffout_default_uses_the_internal_diff_grow_size() {
        let dout = DiffoutT::default();
        assert!(dout.dout_ga.is_empty());
        assert_eq!(dout.dout_ga.ga_growsize, 100);
    }

    #[test]
    fn extract_hunk_internal_returns_hunks_in_order() {
        let first = DiffhunkT {
            lnum_orig: 1,
            count_orig: 2,
            lnum_new: 3,
            count_new: 4,
        };
        let second = DiffhunkT {
            lnum_orig: 8,
            count_orig: 1,
            lnum_new: 9,
            count_new: 0,
        };
        let mut dout = DiffoutT::default();
        dout.dout_ga.items.extend([first, second]);
        let mut idx = 0;
        let mut got = DiffhunkT::default();

        assert!(!extract_hunk_internal(&dout, &mut got, &mut idx));
        assert_eq!(got, first);
        assert_eq!(idx, 1);
        assert!(!extract_hunk_internal(&dout, &mut got, &mut idx));
        assert_eq!(got, second);
        assert_eq!(idx, 2);
    }

    #[test]
    fn extract_hunk_internal_reports_eof_without_changing_outputs() {
        let dout = DiffoutT::default();
        let original = DiffhunkT {
            lnum_orig: 7,
            count_orig: 6,
            lnum_new: 5,
            count_new: 4,
        };
        let mut got = original;
        let mut idx = 0;

        assert!(extract_hunk_internal(&dout, &mut got, &mut idx));
        assert_eq!(got, original);
        assert_eq!(idx, 0);
    }

    #[test]
    fn extract_hunk_internal_remains_at_eof_after_consuming_the_last_hunk() {
        let mut dout = DiffoutT::default();
        dout.dout_ga.items.push(DiffhunkT::default());
        let mut idx = 0;
        let mut got = DiffhunkT::default();
        assert!(!extract_hunk_internal(&dout, &mut got, &mut idx));

        assert!(extract_hunk_internal(&dout, &mut got, &mut idx));
        assert_eq!(idx, 1);
    }

    #[test]
    fn xdiff_out_converts_zero_based_starts_to_one_based_lines() {
        let mut dout = DiffoutT::default();

        assert_eq!(xdiff_out(0, 3, 7, 2, &mut dout), 0);

        assert_eq!(
            dout.dout_ga.items,
            vec![DiffhunkT {
                lnum_orig: 1,
                count_orig: 3,
                lnum_new: 8,
                count_new: 2,
            }]
        );
    }

    #[test]
    fn xdiff_out_appends_each_callback_result() {
        let mut dout = DiffoutT::default();
        xdiff_out(1, 2, 3, 4, &mut dout);
        xdiff_out(10, 0, 20, 5, &mut dout);

        assert_eq!(dout.dout_ga.ga_len(), 2);
        assert_eq!(dout.dout_ga.items[0].lnum_orig, 2);
        assert_eq!(dout.dout_ga.items[1].lnum_orig, 11);
        assert_eq!(dout.dout_ga.items[1].count_orig, 0);
        assert_eq!(dout.dout_ga.items[1].lnum_new, 21);
    }

    #[test]
    fn parse_diff_ed_parses_change_ranges() {
        let mut hunk = DiffhunkT::default();

        assert_eq!(parse_diff_ed(b"3,5c7,9", &mut hunk), crate::vim_defs::OK);
        assert_eq!(
            hunk,
            DiffhunkT {
                lnum_orig: 3,
                count_orig: 3,
                lnum_new: 7,
                count_new: 3,
            }
        );
    }

    #[test]
    fn parse_diff_ed_parses_append_as_an_empty_original_range() {
        let mut hunk = DiffhunkT::default();

        assert_eq!(parse_diff_ed(b"4a8,10", &mut hunk), crate::vim_defs::OK);
        assert_eq!(hunk.lnum_orig, 5);
        assert_eq!(hunk.count_orig, 0);
        assert_eq!(hunk.lnum_new, 8);
        assert_eq!(hunk.count_new, 3);
    }

    #[test]
    fn parse_diff_ed_parses_delete_as_an_empty_new_range() {
        let mut hunk = DiffhunkT::default();

        assert_eq!(parse_diff_ed(b"4,6d8", &mut hunk), crate::vim_defs::OK);
        assert_eq!(hunk.lnum_orig, 4);
        assert_eq!(hunk.count_orig, 3);
        assert_eq!(hunk.lnum_new, 9);
        assert_eq!(hunk.count_new, 0);
    }

    #[test]
    fn parse_diff_ed_rejects_unknown_operators_and_reversed_ranges() {
        let original = DiffhunkT {
            lnum_orig: 99,
            ..Default::default()
        };
        let mut hunk = original;
        assert_eq!(parse_diff_ed(b"3x7", &mut hunk), crate::vim_defs::FAIL);
        assert_eq!(hunk, original, "an invalid operator writes nothing");

        assert_eq!(parse_diff_ed(b"5,3c7", &mut hunk), crate::vim_defs::FAIL);
        assert_eq!(hunk, original, "a reversed range writes nothing");
    }

    #[test]
    fn parse_diff_ed_accepts_single_line_ranges_and_trailing_text() {
        let mut hunk = DiffhunkT::default();
        assert_eq!(
            parse_diff_ed(b"3c7 trailing", &mut hunk),
            crate::vim_defs::OK
        );
        assert_eq!((hunk.lnum_orig, hunk.count_orig), (3, 1));
        assert_eq!((hunk.lnum_new, hunk.count_new), (7, 1));
    }

    // --- clear_diffin / clear_diffout ---

    #[test]
    fn clear_diffin_releases_the_in_memory_buffer_when_there_is_no_temp_file() {
        let mut din = DiffinT {
            din_fname: None,
            din_mmfile: b"line one\nline two\n".to_vec(),
        };
        clear_diffin(&mut din);
        assert!(din.din_mmfile.is_empty());
    }

    #[test]
    fn clear_diffin_deletes_the_temp_file_when_there_is_one() {
        // The branch that matters: with a file named, the FILE is
        // removed rather than the (unused) memory buffer.
        let path = std::env::temp_dir().join("nero_clear_diffin_test.txt");
        std::fs::write(&path, b"scratch").unwrap();
        assert!(path.exists());

        let mut din = DiffinT {
            din_fname: Some(path.to_str().unwrap().as_bytes().to_vec()),
            din_mmfile: Vec::new(),
        };
        clear_diffin(&mut din);

        let gone = !path.exists();
        let _ = std::fs::remove_file(&path);
        assert!(gone, "the temporary file must be deleted");
    }

    #[test]
    fn clear_diffout_releases_the_growable_array_when_there_is_no_temp_file() {
        let mut dout = DiffoutT {
            dout_fname: None,
            dout_ga: crate::garray_defs::TypedGarrayT {
                ga_growsize: 100,
                items: vec![DiffhunkT::default(); 3],
            },
        };
        clear_diffout(&mut dout);
        assert_eq!(dout.dout_ga.ga_len(), 0);
    }

    #[test]
    fn clear_diffout_deletes_the_temp_file_when_there_is_one() {
        let path = std::env::temp_dir().join("nero_clear_diffout_test.txt");
        std::fs::write(&path, b"scratch").unwrap();

        let mut dout = DiffoutT {
            dout_fname: Some(path.to_str().unwrap().as_bytes().to_vec()),
            dout_ga: crate::garray_defs::TypedGarrayT {
                ga_growsize: 100,
                items: vec![DiffhunkT::default(); 3],
            },
        };
        clear_diffout(&mut dout);

        let gone = !path.exists();
        let ga_len = dout.dout_ga.ga_len();
        let _ = std::fs::remove_file(&path);

        assert!(gone, "the temporary file must be deleted");
        assert_eq!(ga_len, 3, "the unused array is left alone on this branch");
    }

    // --- diff_copy_entry ---

    #[test]
    fn diff_copy_entry_with_no_previous_entry_copies_verbatim() {
        // Without a previous entry the offset is zero, so the new slot
        // takes the original's numbers unchanged.
        let mut dp = crate::buffer_defs::DiffT::default();
        dp.df_lnum[0] = 10;
        dp.df_count[0] = 3;

        diff_copy_entry(None, &mut dp, 0, 1);

        assert_eq!(dp.df_lnum[1], 10);
        assert_eq!(dp.df_count[1], 3);
    }

    #[test]
    fn diff_copy_entry_subtracts_the_offset_accumulated_above_it() {
        // The previous entry ends at 5+2=7 in slot 0 but 5+0=5 in
        // slot 1, so slot 1 runs 2 lines "behind": an entry at line 20
        // in slot 0 is line 18 in slot 1.
        let mut prev = crate::buffer_defs::DiffT::default();
        prev.df_lnum[0] = 5;
        prev.df_count[0] = 2;
        prev.df_lnum[1] = 5;
        prev.df_count[1] = 0;

        let mut dp = crate::buffer_defs::DiffT::default();
        dp.df_lnum[0] = 20;
        dp.df_count[0] = 4;

        diff_copy_entry(Some(&prev), &mut dp, 0, 1);

        assert_eq!(dp.df_lnum[1], 18);
        assert_eq!(dp.df_count[1], 4, "the count is copied, never shifted");
    }

    #[test]
    fn diff_copy_entry_handles_a_negative_offset() {
        // The offset runs the other way when the NEW slot is the one
        // that ran ahead, so the copied line number moves down the
        // buffer rather than up.
        let mut prev = crate::buffer_defs::DiffT::default();
        prev.df_lnum[0] = 5;
        prev.df_count[0] = 0;
        prev.df_lnum[1] = 5;
        prev.df_count[1] = 3;

        let mut dp = crate::buffer_defs::DiffT::default();
        dp.df_lnum[0] = 20;
        dp.df_count[0] = 1;

        diff_copy_entry(Some(&prev), &mut dp, 0, 1);

        assert_eq!(dp.df_lnum[1], 23);
        assert_eq!(dp.df_count[1], 1);
    }

    #[test]
    fn diff_copy_entry_leaves_the_source_slot_untouched() {
        let mut dp = crate::buffer_defs::DiffT::default();
        dp.df_lnum[0] = 7;
        dp.df_count[0] = 2;

        diff_copy_entry(None, &mut dp, 0, 2);

        assert_eq!(dp.df_lnum[0], 7);
        assert_eq!(dp.df_count[0], 2);
        assert_eq!(dp.df_lnum[2], 7);
        assert_eq!(dp.df_count[2], 2);
    }

    // --- diff_buf_delete ---

    #[test]
    fn diff_buf_delete_clears_the_slot_and_marks_the_list_outdated() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let buf_ptr: *mut BufT = &mut buf;
        let mut win = crate::buffer_defs::WinT::default();
        let mut tp = crate::buffer_defs::TabpageT::default();
        tp.tp_diffbuf[2] = buf_ptr;
        let tp_ptr: *mut crate::buffer_defs::TabpageT = &mut tp;

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let (pf, pc, pw) = (g.first_tabpage, g.curtab, g.curwin);
        g.first_tabpage = tp_ptr;
        g.curtab = tp_ptr;
        g.curwin = &mut win;
        let prev_redraw = unsafe { *NEED_DIFF_REDRAW.get_mut() };
        unsafe { *NEED_DIFF_REDRAW.get_mut() = false };

        unsafe { diff_buf_delete(buf_ptr) };

        assert!(unsafe { (*tp_ptr).tp_diffbuf[2] }.is_null());
        assert_ne!(unsafe { (*tp_ptr).tp_diff_invalid }, 0);
        assert!(
            unsafe { *NEED_DIFF_REDRAW.get_mut() },
            "the current tab page defers a redraw"
        );

        unsafe { *NEED_DIFF_REDRAW.get_mut() = prev_redraw };
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.first_tabpage = pf;
        g.curtab = pc;
        g.curwin = pw;
    }

    #[test]
    fn diff_buf_delete_skips_tab_pages_that_never_held_the_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut other = BufT::default();
        let buf_ptr: *mut BufT = &mut buf;
        let mut win = crate::buffer_defs::WinT::default();

        let mut tp = crate::buffer_defs::TabpageT::default();
        tp.tp_diffbuf[0] = &mut other;
        let tp_ptr: *mut crate::buffer_defs::TabpageT = &mut tp;

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let (pf, pc, pw) = (g.first_tabpage, g.curtab, g.curwin);
        g.first_tabpage = tp_ptr;
        g.curtab = tp_ptr;
        g.curwin = &mut win;
        let prev_redraw = unsafe { *NEED_DIFF_REDRAW.get_mut() };
        unsafe { *NEED_DIFF_REDRAW.get_mut() = false };

        unsafe { diff_buf_delete(buf_ptr) };

        assert!(!unsafe { (*tp_ptr).tp_diffbuf[0] }.is_null(), "left alone");
        assert_eq!(unsafe { (*tp_ptr).tp_diff_invalid }, 0);
        assert!(!unsafe { *NEED_DIFF_REDRAW.get_mut() }, "no redraw requested");

        unsafe { *NEED_DIFF_REDRAW.get_mut() = prev_redraw };
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.first_tabpage = pf;
        g.curtab = pc;
        g.curwin = pw;
    }

    #[test]
    fn diff_buf_delete_walks_every_tab_page_but_defers_redraw_only_for_curtab() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let buf_ptr: *mut BufT = &mut buf;
        let mut win = crate::buffer_defs::WinT::default();

        let mut second = crate::buffer_defs::TabpageT::default();
        second.tp_diffbuf[1] = buf_ptr;
        let second_ptr: *mut crate::buffer_defs::TabpageT = &mut second;
        let mut first = crate::buffer_defs::TabpageT::default();
        first.tp_diffbuf[0] = buf_ptr;
        first.tp_next = second_ptr;
        let first_ptr: *mut crate::buffer_defs::TabpageT = &mut first;

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let (pf, pc, pw) = (g.first_tabpage, g.curtab, g.curwin);
        g.first_tabpage = first_ptr;
        // Only the SECOND tab page is current.
        g.curtab = second_ptr;
        g.curwin = &mut win;
        let prev_redraw = unsafe { *NEED_DIFF_REDRAW.get_mut() };
        unsafe { *NEED_DIFF_REDRAW.get_mut() = false };

        unsafe { diff_buf_delete(buf_ptr) };

        // Both tab pages lost the buffer...
        assert!(unsafe { (*first_ptr).tp_diffbuf[0] }.is_null());
        assert!(unsafe { (*second_ptr).tp_diffbuf[1] }.is_null());
        assert_ne!(unsafe { (*first_ptr).tp_diff_invalid }, 0);
        assert_ne!(unsafe { (*second_ptr).tp_diff_invalid }, 0);
        // ...and the redraw came from the current one.
        assert!(unsafe { *NEED_DIFF_REDRAW.get_mut() });

        unsafe { *NEED_DIFF_REDRAW.get_mut() = prev_redraw };
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.first_tabpage = pf;
        g.curtab = pc;
        g.curwin = pw;
    }

    // --- diff_free ---

    #[test]
    fn diff_free_of_the_head_promotes_its_successor() {
        let mut tp = crate::buffer_defs::TabpageT::default();
        let second = unsafe { diff_alloc_new(&mut tp, std::ptr::null_mut(), std::ptr::null_mut()) };
        let first = unsafe { diff_alloc_new(&mut tp, std::ptr::null_mut(), second) };
        assert!(std::ptr::eq(tp.tp_first_diff, first));

        let ret = unsafe { diff_free(&mut tp, std::ptr::null_mut(), first) };

        assert!(std::ptr::eq(ret, second), "the successor is returned");
        assert!(std::ptr::eq(tp.tp_first_diff, second), "and becomes the head");

        unsafe { diff_clear(&mut tp) };
    }

    #[test]
    fn diff_free_of_a_middle_block_relinks_around_it() {
        let mut tp = crate::buffer_defs::TabpageT::default();
        let third = unsafe { diff_alloc_new(&mut tp, std::ptr::null_mut(), std::ptr::null_mut()) };
        let second = unsafe { diff_alloc_new(&mut tp, std::ptr::null_mut(), third) };
        let first = unsafe { diff_alloc_new(&mut tp, std::ptr::null_mut(), second) };

        let ret = unsafe { diff_free(&mut tp, first, second) };

        assert!(std::ptr::eq(ret, third));
        assert!(std::ptr::eq(tp.tp_first_diff, first), "head is unchanged");
        unsafe { assert!(std::ptr::eq((*first).df_next, third), "spliced around") };

        unsafe { diff_clear(&mut tp) };
    }

    #[test]
    fn diff_free_of_the_only_block_empties_the_chain() {
        let mut tp = crate::buffer_defs::TabpageT::default();
        let only = unsafe { diff_alloc_new(&mut tp, std::ptr::null_mut(), std::ptr::null_mut()) };

        let ret = unsafe { diff_free(&mut tp, std::ptr::null_mut(), only) };

        assert!(ret.is_null());
        assert!(tp.tp_first_diff.is_null());
    }

    // --- diff_alloc_new ---

    #[test]
    fn diff_alloc_new_with_no_previous_becomes_the_chain_head() {
        let mut tp = crate::buffer_defs::TabpageT::default();
        let dnew =
            unsafe { diff_alloc_new(&mut tp, std::ptr::null_mut(), std::ptr::null_mut()) };

        assert!(std::ptr::eq(tp.tp_first_diff, dnew));
        unsafe {
            assert!((*dnew).df_next.is_null());
            assert!(!(*dnew).is_linematched);
            assert!(!(*dnew).has_changes);
        }

        unsafe { diff_clear(&mut tp) };
    }

    #[test]
    fn diff_alloc_new_links_after_the_previous_block() {
        let mut tp = crate::buffer_defs::TabpageT::default();
        let first = unsafe { diff_alloc_new(&mut tp, std::ptr::null_mut(), std::ptr::null_mut()) };
        let second = unsafe { diff_alloc_new(&mut tp, first, std::ptr::null_mut()) };

        assert!(std::ptr::eq(tp.tp_first_diff, first), "head is unchanged");
        unsafe {
            assert!(std::ptr::eq((*first).df_next, second));
            assert!((*second).df_next.is_null());
        }

        unsafe { diff_clear(&mut tp) };
    }

    #[test]
    fn diff_alloc_new_splices_in_front_of_an_existing_block() {
        // Insert between: dprev -> new -> dp.
        let mut tp = crate::buffer_defs::TabpageT::default();
        let last = unsafe { diff_alloc_new(&mut tp, std::ptr::null_mut(), std::ptr::null_mut()) };
        let middle = unsafe { diff_alloc_new(&mut tp, std::ptr::null_mut(), last) };

        // `middle` took the head slot and points at `last`.
        assert!(std::ptr::eq(tp.tp_first_diff, middle));
        unsafe { assert!(std::ptr::eq((*middle).df_next, last)) };

        unsafe { diff_clear(&mut tp) };
    }

    // --- diff_check_sanity ---

    #[test]
    fn diff_check_sanity_accepts_a_range_inside_the_buffer() {
        let mut buf = BufT::default();
        buf.b_ml.ml_line_count = 10;
        let mut tp = crate::buffer_defs::TabpageT::default();
        tp.tp_diffbuf[0] = &mut buf;

        // Lines 3..=6, well inside a 10-line buffer.
        let mut dp = crate::buffer_defs::DiffT::default();
        dp.df_lnum[0] = 3;
        dp.df_count[0] = 4;

        assert_eq!(unsafe { diff_check_sanity(&tp, &dp) }, crate::vim_defs::OK);
    }

    #[test]
    fn diff_check_sanity_accepts_a_range_ending_exactly_at_the_last_line() {
        let mut buf = BufT::default();
        buf.b_ml.ml_line_count = 10;
        let mut tp = crate::buffer_defs::TabpageT::default();
        tp.tp_diffbuf[0] = &mut buf;

        // df_lnum + df_count - 1 == 10, the last valid line.
        let mut dp = crate::buffer_defs::DiffT::default();
        dp.df_lnum[0] = 8;
        dp.df_count[0] = 3;

        assert_eq!(unsafe { diff_check_sanity(&tp, &dp) }, crate::vim_defs::OK);
    }

    #[test]
    fn diff_check_sanity_rejects_a_range_past_the_end() {
        let mut buf = BufT::default();
        buf.b_ml.ml_line_count = 10;
        let mut tp = crate::buffer_defs::TabpageT::default();
        tp.tp_diffbuf[0] = &mut buf;

        let mut dp = crate::buffer_defs::DiffT::default();
        dp.df_lnum[0] = 8;
        dp.df_count[0] = 4; // reaches line 11

        assert_eq!(unsafe { diff_check_sanity(&tp, &dp) }, crate::vim_defs::FAIL);
    }

    #[test]
    fn diff_check_sanity_skips_slots_with_no_registered_buffer() {
        // Only slot 1 has a buffer; slot 0's nonsense range must be
        // ignored entirely rather than dereferenced.
        let mut buf = BufT::default();
        buf.b_ml.ml_line_count = 10;
        let mut tp = crate::buffer_defs::TabpageT::default();
        tp.tp_diffbuf[1] = &mut buf;

        let mut dp = crate::buffer_defs::DiffT::default();
        dp.df_lnum[0] = 999;
        dp.df_count[0] = 999;
        dp.df_lnum[1] = 1;
        dp.df_count[1] = 2;

        assert_eq!(unsafe { diff_check_sanity(&tp, &dp) }, crate::vim_defs::OK);
    }

    #[test]
    fn diff_check_sanity_checks_every_registered_buffer() {
        // The first buffer is fine, the second is not - the failure
        // must still be reported.
        let mut ok_buf = BufT::default();
        ok_buf.b_ml.ml_line_count = 10;
        let mut short_buf = BufT::default();
        short_buf.b_ml.ml_line_count = 2;
        let mut tp = crate::buffer_defs::TabpageT::default();
        tp.tp_diffbuf[0] = &mut ok_buf;
        tp.tp_diffbuf[1] = &mut short_buf;

        let mut dp = crate::buffer_defs::DiffT::default();
        dp.df_lnum[0] = 1;
        dp.df_count[0] = 3;
        dp.df_lnum[1] = 1;
        dp.df_count[1] = 5; // reaches line 5 of a 2-line buffer

        assert_eq!(unsafe { diff_check_sanity(&tp, &dp) }, crate::vim_defs::FAIL);
    }

    /// Allocates a diff block chain of `n` blocks, returning the head.
    fn alloc_diff_chain(n: usize) -> *mut crate::buffer_defs::DiffT {
        let mut head = std::ptr::null_mut();
        for i in (0..n).rev() {
            let block = Box::new(crate::buffer_defs::DiffT {
                df_next: head,
                df_lnum: [crate::pos_defs::LinenrT::try_from(i).unwrap();
                    crate::buffer_defs::DB_COUNT],
                ..Default::default()
            });
            head = Box::into_raw(block);
        }
        head
    }

    #[test]
    fn lnum_compare_orders_line_numbers() {
        use std::cmp::Ordering;
        assert_eq!(lnum_compare(1, 2), Ordering::Less);
        assert_eq!(lnum_compare(2, 1), Ordering::Greater);
        assert_eq!(lnum_compare(3, 3), Ordering::Equal);
    }

    #[test]
    fn lnum_compare_sorts_a_list() {
        let mut lnums = vec![5, 1, 3, 2, 4];
        lnums.sort_by(|a, b| lnum_compare(*a, *b));
        assert_eq!(lnums, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn clear_diffblock_on_null_is_a_no_op() {
        unsafe { clear_diffblock(std::ptr::null_mut()) };
    }

    #[test]
    fn diff_clear_frees_the_whole_chain() {
        let mut tp = crate::buffer_defs::TabpageT {
            tp_first_diff: alloc_diff_chain(4),
            ..Default::default()
        };
        assert!(!tp.tp_first_diff.is_null());

        unsafe { diff_clear(&mut tp) };

        // The head is cleared, so a later walk sees no blocks at all.
        assert!(tp.tp_first_diff.is_null());
    }

    #[test]
    fn diff_clear_on_an_empty_tabpage_is_a_no_op() {
        let mut tp = crate::buffer_defs::TabpageT::default();
        unsafe { diff_clear(&mut tp) };
        assert!(tp.tp_first_diff.is_null());
    }

    #[test]
    fn diff_clear_handles_a_single_block() {
        let mut tp = crate::buffer_defs::TabpageT {
            tp_first_diff: alloc_diff_chain(1),
            ..Default::default()
        };
        unsafe { diff_clear(&mut tp) };
        assert!(tp.tp_first_diff.is_null());
    }

    #[test]
    fn diff_flags_default_matches_the_real_diffopt_default() {
        // "internal,filler,closeoff,indent-heuristic,inline:char,
        // linematch:40" - matching diff.c's own static initializer.
        // Must hold the lock: DIFF_FLAGS is shared GlobalCell state
        // that other tests in this module temporarily mutate (see
        // diffopt_filler_false_when_flag_cleared/
        // diff_check_fill_returns_zero_when_diffopt_filler_disabled).
        let _lock = crate::globals::global_state_test_lock();
        let flags = unsafe { *DIFF_FLAGS.get_mut() };
        assert_eq!(
            flags,
            diff_flag::INTERNAL
                | diff_flag::FILLER
                | diff_flag::CLOSE_OFF
                | diff_flag::LINEMATCH
                | diff_flag::INLINE_CHAR
        );
    }

    #[test]
    fn diffopt_filler_true_by_default() {
        // See diff_flags_default_matches_the_real_diffopt_default's
        // own comment for why this lock is required.
        let _lock = crate::globals::global_state_test_lock();
        assert!(diffopt_filler());
    }

    #[test]
    fn diffopt_closeoff_true_by_default() {
        // See diff_flags_default_matches_the_real_diffopt_default's
        // own comment for why this lock is required.
        let _lock = crate::globals::global_state_test_lock();
        assert!(diffopt_closeoff());
    }

    #[test]
    fn diffopt_filler_false_when_flag_cleared() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { *DIFF_FLAGS.get_mut() };
        unsafe { *DIFF_FLAGS.get_mut() &= !diff_flag::FILLER };
        assert!(!diffopt_filler());
        unsafe { *DIFF_FLAGS.get_mut() = prev };
    }

    #[test]
    fn diff_internal_true_by_default() {
        // 'diffopt' defaults to including "internal", and 'diffexpr'
        // defaults to empty, so the internal algorithm is used.
        let _lock = crate::globals::global_state_test_lock();
        assert!(diff_internal());
    }

    #[test]
    fn diff_internal_false_without_the_internal_flag() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { *DIFF_FLAGS.get_mut() };
        unsafe { *DIFF_FLAGS.get_mut() &= !diff_flag::INTERNAL };
        assert!(!diff_internal());
        unsafe { *DIFF_FLAGS.get_mut() = prev };
    }

    #[test]
    fn diff_internal_false_when_diffexpr_is_set() {
        // A non-empty 'diffexpr' means the user wants their own
        // command run instead, even with "internal" still listed.
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_dex.clone();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_dex = Some(b"MyDiff()".to_vec());
        assert!(!diff_internal());

        // An empty string counts as unset, same as the original's
        // own `*p_dex == NUL` check.
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_dex = Some(Vec::new());
        assert!(diff_internal());

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_dex = prev;
    }

    #[test]
    fn diffopt_horizontal_false_by_default() {
        // "horizontal" is NOT part of the real 'diffopt' default
        // string - see diff_flags_default_matches_the_real_diffopt_
        // default's own comment for why this lock is required.
        let _lock = crate::globals::global_state_test_lock();
        assert!(!diffopt_horizontal());
    }

    #[test]
    fn diffopt_horizontal_true_when_flag_set() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { *DIFF_FLAGS.get_mut() };
        unsafe { *DIFF_FLAGS.get_mut() |= diff_flag::HORIZONTAL };
        assert!(diffopt_horizontal());
        unsafe { *DIFF_FLAGS.get_mut() = prev };
    }

    #[test]
    fn diffopt_hiddenoff_false_by_default() {
        // "hiddenoff" is NOT part of the real 'diffopt' default
        // string either - same locking rationale as above.
        let _lock = crate::globals::global_state_test_lock();
        assert!(!diffopt_hiddenoff());
    }

    #[test]
    fn diffopt_hiddenoff_true_when_flag_set() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { *DIFF_FLAGS.get_mut() };
        unsafe { *DIFF_FLAGS.get_mut() |= diff_flag::HIDDEN_OFF };
        assert!(diffopt_hiddenoff());
        unsafe { *DIFF_FLAGS.get_mut() = prev };
    }

    // --- diff_equal_char ---

    #[test]
    fn diff_equal_char_matching_ascii_bytes() {
        let _lock = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { diff_equal_char(b"a\0", b"a\0") }, Some(1));
    }

    #[test]
    fn diff_equal_char_mismatched_ascii_bytes_case_sensitive_by_default() {
        let _lock = crate::globals::global_state_test_lock();
        assert_eq!(unsafe { diff_equal_char(b"A\0", b"a\0") }, None);
    }

    #[test]
    fn diff_equal_char_mismatched_case_matches_when_icase_flag_set() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { *DIFF_FLAGS.get_mut() };
        unsafe { *DIFF_FLAGS.get_mut() |= diff_flag::ICASE };
        let result = unsafe { diff_equal_char(b"A\0", b"a\0") };
        unsafe { *DIFF_FLAGS.get_mut() = prev };
        assert_eq!(result, Some(1));
    }

    #[test]
    fn diff_equal_char_different_byte_lengths_never_match() {
        // A 3-byte CJK character vs. a 1-byte ASCII character - the
        // utfc_ptr2len mismatch itself short-circuits before any
        // byte/char comparison.
        let _lock = crate::globals::global_state_test_lock();
        let word = "日\0".as_bytes();
        assert_eq!(unsafe { diff_equal_char(word, b"a\0") }, None);
    }

    #[test]
    fn diff_equal_char_matching_multibyte_characters() {
        let _lock = crate::globals::global_state_test_lock();
        let word = "日\0".as_bytes();
        assert_eq!(unsafe { diff_equal_char(word, word) }, Some(3));
    }

    #[test]
    fn diff_equal_char_different_multibyte_characters_same_length() {
        let _lock = crate::globals::global_state_test_lock();
        let a = "日\0".as_bytes();
        let b = "本\0".as_bytes();
        assert_eq!(unsafe { diff_equal_char(a, b) }, None);
    }

    /// Points `GLOBALS.curtab` at `tp` for the guard's lifetime,
    /// restoring the previous value on drop. Callers must hold
    /// `global_state_test_lock()` for the guard's whole lifetime.
    struct CurtabGuard {
        previous: *mut crate::buffer_defs::TabpageT,
    }

    impl CurtabGuard {
        fn set(new_curtab: *mut crate::buffer_defs::TabpageT) -> Self {
            let previous = unsafe { crate::globals::GLOBALS.get_mut() }.curtab;
            unsafe { crate::globals::GLOBALS.get_mut() }.curtab = new_curtab;
            CurtabGuard { previous }
        }
    }

    impl Drop for CurtabGuard {
        fn drop(&mut self) {
            unsafe { crate::globals::GLOBALS.get_mut() }.curtab = self.previous;
        }
    }

    #[test]
    fn diff_check_with_linestatus_returns_zero_when_no_diffs_at_all() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);

        let wp = WinT::default();
        let mut linestatus = 42;
        assert_eq!(
            unsafe { diff_check_with_linestatus(&wp, 1, Some(&mut linestatus)) },
            0
        );
        assert_eq!(linestatus, 0);
    }

    #[test]
    fn diff_check_with_linestatus_returns_zero_when_window_not_in_diff_mode() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);

        let wp = WinT { w_onebuf_opt: crate::buffer_defs::WinoptT { wo_diff: 0, ..Default::default() }, ..Default::default() };
        assert_eq!(unsafe { diff_check_with_linestatus(&wp, 1, None) }, 0);
    }

    #[test]
    fn diff_check_fill_returns_zero_when_diffopt_filler_disabled() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { *DIFF_FLAGS.get_mut() };
        unsafe { *DIFF_FLAGS.get_mut() &= !diff_flag::FILLER };

        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);
        let wp = WinT::default();
        assert_eq!(unsafe { diff_check_fill(&wp, 1) }, 0);

        unsafe { *DIFF_FLAGS.get_mut() = prev };
    }

    #[test]
    fn diff_check_fill_returns_zero_via_no_diffs_fast_path() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);
        let wp = WinT::default();
        // diffopt_filler() is true by default, so this exercises
        // diff_check_with_linestatus's own "no diffs at all" path.
        assert!(diffopt_filler());
        assert_eq!(unsafe { diff_check_fill(&wp, 1) }, 0);
    }

    #[test]
    #[should_panic(expected = "ex_diffupdate")]
    fn diff_check_with_linestatus_panics_when_tp_diff_invalid_is_set() {
        // Not achievable via any real translated function yet (nothing
        // can set tp_diff_invalid) - pokes it directly to prove the
        // real, faithfully-translated check is in place, independent
        // of how tp_diff_invalid eventually gets set.
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT { tp_diff_invalid: 1, ..Default::default() };
        let _guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);
        let wp = WinT::default();
        let _ = unsafe { diff_check_with_linestatus(&wp, 1, None) };
    }

    #[test]
    fn diff_buf_idx_finds_a_registered_buffer() {
        let mut buf = BufT::default();
        let buf_ptr = &mut buf as *mut BufT;
        let mut tp = crate::buffer_defs::TabpageT::default();
        tp.tp_diffbuf[2] = buf_ptr;
        assert_eq!(diff_buf_idx(buf_ptr, &mut tp as *mut crate::buffer_defs::TabpageT), 2);
    }

    #[test]
    fn diff_buf_idx_returns_db_count_when_not_registered() {
        let mut buf = BufT::default();
        let mut other = BufT::default();
        let mut tp = crate::buffer_defs::TabpageT::default();
        tp.tp_diffbuf[0] = &mut other as *mut BufT;
        assert_eq!(
            diff_buf_idx(&mut buf as *mut BufT, &mut tp as *mut crate::buffer_defs::TabpageT),
            crate::buffer_defs::DB_COUNT
        );
    }

    #[test]
    fn valid_diff_false_when_tp_first_diff_is_null() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);
        let some_diff = std::ptr::null::<crate::buffer_defs::DiffT>().wrapping_add(1);
        assert!(!unsafe { valid_diff(some_diff) });
    }

    #[test]
    fn valid_diff_true_when_the_pointer_is_the_only_entry() {
        let _lock = crate::globals::global_state_test_lock();
        let mut d1 = crate::buffer_defs::DiffT::default();
        let d1_ptr = &mut d1 as *mut crate::buffer_defs::DiffT;
        let mut tp = crate::buffer_defs::TabpageT { tp_first_diff: d1_ptr, ..Default::default() };
        let _guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);
        assert!(unsafe { valid_diff(d1_ptr) });
    }

    #[test]
    fn valid_diff_true_when_found_later_in_the_list() {
        let _lock = crate::globals::global_state_test_lock();
        let mut d2 = crate::buffer_defs::DiffT::default();
        let d2_ptr = &mut d2 as *mut crate::buffer_defs::DiffT;
        let mut d1 = crate::buffer_defs::DiffT { df_next: d2_ptr, ..Default::default() };
        let d1_ptr = &mut d1 as *mut crate::buffer_defs::DiffT;
        let mut tp = crate::buffer_defs::TabpageT { tp_first_diff: d1_ptr, ..Default::default() };
        let _guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);
        assert!(unsafe { valid_diff(d2_ptr) });
    }

    #[test]
    fn valid_diff_false_when_not_present_in_a_nonempty_list() {
        let _lock = crate::globals::global_state_test_lock();
        let mut d1 = crate::buffer_defs::DiffT::default();
        let d1_ptr = &mut d1 as *mut crate::buffer_defs::DiffT;
        let mut tp = crate::buffer_defs::TabpageT { tp_first_diff: d1_ptr, ..Default::default() };
        let _guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);
        let mut other = crate::buffer_defs::DiffT::default();
        assert!(!unsafe { valid_diff(&mut other as *mut crate::buffer_defs::DiffT) });
    }

    /// Points `GLOBALS.first_tabpage` at `head` for the guard's
    /// lifetime, restoring the previous value on drop. Callers must
    /// hold `global_state_test_lock()` for the guard's whole lifetime.
    struct FirstTabpageGuard {
        previous: *mut crate::buffer_defs::TabpageT,
    }

    impl FirstTabpageGuard {
        fn set(head: *mut crate::buffer_defs::TabpageT) -> Self {
            let previous = unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage;
            unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage = head;
            FirstTabpageGuard { previous }
        }
    }

    impl Drop for FirstTabpageGuard {
        fn drop(&mut self) {
            unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage = self.previous;
        }
    }

    #[test]
    fn diff_mode_buf_true_when_registered_in_a_non_first_tabpage() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let buf_ptr = &mut buf as *mut BufT;
        let mut tp2 = crate::buffer_defs::TabpageT::default();
        tp2.tp_diffbuf[0] = buf_ptr;
        let mut tp1 = crate::buffer_defs::TabpageT {
            tp_next: &mut tp2 as *mut crate::buffer_defs::TabpageT,
            ..Default::default()
        };
        let _guard = FirstTabpageGuard::set(&mut tp1 as *mut crate::buffer_defs::TabpageT);

        assert!(unsafe { diff_mode_buf(buf_ptr) });
    }

    #[test]
    fn diff_mode_buf_false_when_not_registered_anywhere() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _guard = FirstTabpageGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);

        assert!(!unsafe { diff_mode_buf(&mut buf as *mut BufT) });
    }

    #[test]
    fn diff_mark_adjust_is_a_no_op_when_no_tabpage_has_buf_registered() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut tp2 = crate::buffer_defs::TabpageT::default();
        let mut tp1 = crate::buffer_defs::TabpageT {
            tp_next: &mut tp2 as *mut crate::buffer_defs::TabpageT,
            ..Default::default()
        };
        let _guard = FirstTabpageGuard::set(&mut tp1 as *mut crate::buffer_defs::TabpageT);

        // Must walk both tabpages without panicking, since neither has
        // `buf` registered in its own tp_diffbuf[].
        unsafe { diff_mark_adjust(&mut buf as *mut BufT, 1, 5, 2, 0) };
    }

    #[test]
    fn diff_mark_adjust_is_a_no_op_when_the_tabpage_list_is_empty() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let _guard = FirstTabpageGuard::set(std::ptr::null_mut());
        unsafe { diff_mark_adjust(&mut buf as *mut BufT, 1, 5, 2, 0) };
    }

    #[test]
    #[should_panic(expected = "diff_buf_idx never returns anything but DB_COUNT today")]
    fn diff_mark_adjust_panics_when_buf_is_actually_registered() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let buf_ptr = &mut buf as *mut BufT;
        let mut tp = crate::buffer_defs::TabpageT::default();
        tp.tp_diffbuf[1] = buf_ptr;
        let _guard = FirstTabpageGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);

        unsafe { diff_mark_adjust(buf_ptr, 1, 5, 2, 0) };
    }

    /// Points `GLOBALS.curbuf` at `buf` for the guard's lifetime,
    /// restoring the previous value on drop. Callers must hold
    /// `global_state_test_lock()` for the guard's whole lifetime
    /// (matches this file's own `CurtabGuard`/`FirstTabpageGuard`
    /// convention: does NOT acquire its own lock).
    struct CurbufGuard {
        previous: *mut BufT,
    }

    impl CurbufGuard {
        fn set(new_curbuf: *mut BufT) -> Self {
            let previous = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
            unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = new_curbuf;
            CurbufGuard { previous }
        }
    }

    impl Drop for CurbufGuard {
        fn drop(&mut self) {
            unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = self.previous;
        }
    }

    #[test]
    fn diff_get_corresponding_line_returns_lnum_unchanged_via_no_diffs_fast_path() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf1 = BufT::default();
        let mut curbuf = BufT {
            b_ml: crate::memline_defs::MemlineT { ml_line_count: 100, ..Default::default() },
            ..Default::default()
        };
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _curtab_guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);
        let _curbuf_guard = CurbufGuard::set(&mut curbuf as *mut BufT);

        assert_eq!(unsafe { diff_get_corresponding_line(&mut buf1 as *mut BufT, 42) }, 42);
    }

    #[test]
    fn diff_get_corresponding_line_clamps_to_the_buffers_own_line_count() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf1 = BufT::default();
        let mut curbuf = BufT {
            b_ml: crate::memline_defs::MemlineT { ml_line_count: 10, ..Default::default() },
            ..Default::default()
        };
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _curtab_guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);
        let _curbuf_guard = CurbufGuard::set(&mut curbuf as *mut BufT);

        // lnum1 (999) exceeds curbuf's own line count (10) - clamped.
        assert_eq!(unsafe { diff_get_corresponding_line(&mut buf1 as *mut BufT, 999) }, 10);
    }

    #[test]
    fn diff_lnum_win_returns_zero_via_no_diffs_fast_path() {
        let _lock = crate::globals::global_state_test_lock();
        let mut curbuf = BufT::default();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _curtab_guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);
        let _curbuf_guard = CurbufGuard::set(&mut curbuf as *mut BufT);

        let mut wp = WinT::default();
        assert_eq!(unsafe { diff_lnum_win(5, &mut wp as *mut WinT) }, 0);
    }

    #[test]
    fn diff_move_to_returns_fail_via_no_diffs_fast_path() {
        let _lock = crate::globals::global_state_test_lock();
        let mut curbuf = BufT::default();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _curtab_guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);
        let _curbuf_guard = CurbufGuard::set(&mut curbuf as *mut BufT);

        assert_eq!(unsafe { diff_move_to(1, 1) }, crate::vim_defs::FAIL);
    }

    #[test]
    #[should_panic(expected = "the real diff-block search/cursor-move logic is not yet translated")]
    fn diff_move_to_panics_when_a_real_diff_search_would_be_needed() {
        let _lock = crate::globals::global_state_test_lock();
        let mut curbuf = BufT::default();
        let buf_ptr = &mut curbuf as *mut BufT;
        let mut dp = crate::buffer_defs::DiffT {
            df_next: std::ptr::null_mut(),
            df_lnum: [0; crate::buffer_defs::DB_COUNT],
            df_count: [0; crate::buffer_defs::DB_COUNT],
            is_linematched: false,
            has_changes: false,
            df_changes: crate::garray_defs::GarrayT::default(),
        };
        let mut tp = crate::buffer_defs::TabpageT::default();
        tp.tp_diffbuf[0] = buf_ptr;
        tp.tp_first_diff = &mut dp as *mut crate::buffer_defs::DiffT;
        let _curtab_guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);
        let _curbuf_guard = CurbufGuard::set(buf_ptr);

        let _ = unsafe { diff_move_to(1, 1) };
    }

    #[test]
    fn diff_update_line_returns_early_when_no_inline_diff_flags_are_set() {
        let _lock = crate::globals::global_state_test_lock();
        let prev = unsafe { *DIFF_FLAGS.get_mut() };
        unsafe { *DIFF_FLAGS.get_mut() &= !ALL_INLINE_DIFF };
        // curbuf/curtab left at their bare defaults - if this reached
        // the diff_buf_idx check (or beyond) it would either return
        // silently anyway or panic; the point of THIS test is that the
        // very first check alone already returns, before either.
        unsafe { diff_update_line(1) };
        unsafe { *DIFF_FLAGS.get_mut() = prev };
    }

    #[test]
    fn diff_update_line_is_a_noop_when_curbuf_is_not_a_diff_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        // Default DIFF_FLAGS already has DIFF_INLINE_CHAR set, so this
        // exercises the SECOND check (diff_buf_idx == DB_COUNT), not
        // the first.
        assert_ne!(unsafe { *DIFF_FLAGS.get_mut() } & ALL_INLINE_DIFF, 0);
        let mut curbuf = BufT::default();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _curtab_guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);
        let _curbuf_guard = CurbufGuard::set(&mut curbuf as *mut BufT);

        unsafe { diff_update_line(1) };
    }

    #[test]
    #[should_panic(expected = "the real diff-block search is not yet translated")]
    fn diff_update_line_panics_when_curbuf_is_a_diff_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let mut curbuf = BufT::default();
        let buf_ptr = &mut curbuf as *mut BufT;
        let mut tp = crate::buffer_defs::TabpageT::default();
        tp.tp_diffbuf[0] = buf_ptr;
        let _curtab_guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);
        let _curbuf_guard = CurbufGuard::set(buf_ptr);

        unsafe { diff_update_line(1) };
    }

    #[test]
    fn diff_infold_false_when_diff_option_is_off() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _curtab_guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);
        let wp = WinT {
            w_onebuf_opt: crate::buffer_defs::WinoptT { wo_diff: 0, ..Default::default() },
            ..Default::default()
        };
        assert!(!unsafe { diff_infold(&wp, 1) });
    }

    #[test]
    fn diff_infold_false_when_window_buffer_is_not_registered() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let mut tp = crate::buffer_defs::TabpageT::default();
        let _curtab_guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);
        let wp = WinT {
            w_buffer: &mut buf as *mut BufT,
            w_onebuf_opt: crate::buffer_defs::WinoptT { wo_diff: 1, ..Default::default() },
            ..Default::default()
        };
        assert!(!unsafe { diff_infold(&wp, 1) });
    }

    #[test]
    fn diff_infold_false_when_no_other_buffer_is_registered() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        let buf_ptr = &mut buf as *mut BufT;
        let mut tp = crate::buffer_defs::TabpageT::default();
        tp.tp_diffbuf[0] = buf_ptr;
        let _curtab_guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);
        let wp = WinT {
            w_buffer: buf_ptr,
            w_onebuf_opt: crate::buffer_defs::WinoptT { wo_diff: 1, ..Default::default() },
            ..Default::default()
        };
        assert!(!unsafe { diff_infold(&wp, 1) });
    }

    #[test]
    #[should_panic(expected = "the real diff-block search is not yet translated")]
    fn diff_infold_panics_when_both_idx_and_other_are_found() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf1 = BufT::default();
        let mut buf2 = BufT::default();
        let buf1_ptr = &mut buf1 as *mut BufT;
        let buf2_ptr = &mut buf2 as *mut BufT;
        let mut tp = crate::buffer_defs::TabpageT::default();
        tp.tp_diffbuf[0] = buf1_ptr;
        tp.tp_diffbuf[1] = buf2_ptr;
        let _curtab_guard = CurtabGuard::set(&mut tp as *mut crate::buffer_defs::TabpageT);
        let wp = WinT {
            w_buffer: buf1_ptr,
            w_onebuf_opt: crate::buffer_defs::WinoptT { wo_diff: 1, ..Default::default() },
            ..Default::default()
        };
        let _ = unsafe { diff_infold(&wp, 1) };
    }
}
