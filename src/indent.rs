//! Translated from `src/nvim/indent.c` (tractable core only).
//!
//! `indent.c` (~2000 lines) is the auto-indent/`'shiftwidth'`/tab-stop
//! computation file. Most of it needs real buffer-modification
//! (`ml_replace`/`changed_bytes`) plus the C-indent (`indent_c.c`) and
//! Lisp-indent engines.
//!
//! Translated: `tabstop_set`/`tabstop_padding`/`tabstop_at`/
//! `tabstop_start`/`tabstop_fromto`, `indent_size_no_ts`/
//! `indent_size_ts` (needed by `plines.c`'s tab-width calculations and
//! by `get_breakindent_win` below); `get_breakindent_win` (needed
//! `buffer.c`'s `buf_get_changedtick`, now tractable since
//! `eval/typval_defs.rs`'s `TypvalT` is real - see that function's own
//! doc comment for its one deliberate gap, `'breakindentopt'="list"`,
//! which needs the real regex engine); `get_indent`/`get_indent_lnum`/
//! `get_indent_buf` (thin wrappers around `indent_size_ts` - needed
//! only `cursor.rs`'s `get_cursor_line_ptr`/`memline.rs`'s `ml_get`/
//! `ml_get_buf`, all already real); `get_sw_value`/`get_sw_value_col`/
//! `get_sw_value_pos`/`get_sw_value_indent`/`get_sts_value` (the
//! effective `'shiftwidth'`/`'softtabstop'` values, needed by
//! `eval/funcs.c`'s `shiftwidth()` and, ahead of their own real
//! callers - `insert.c`'s Insert-mode key handling and `ops.c`'s
//! `shift_block`, neither translated - by the tab-stop-family
//! functions above); `inindent`/`may_do_si` (small self-contained
//! predicates, translated ahead of their own real callers -
//! `insert.c`/`ops.c`/`textobject.c`/`insexpand.c`, none translated -
//! matching the same precedent). `tabstop_set` (the `'vartabstop'`/
//! `'varsofttabstop'` string parser) has 2 real callers now
//! (`optionstr.rs`'s `did_set_varsofttabstop`/`did_set_vartabstop`) -
//! `indent.c`'s own `ex_retab` is still not translated - returns
//! `Result<Option<Vec<ColnrT>>, ()>` instead of a `bool` return
//! plus a `colnr_T **` out-parameter.
//!
//! `tabstop_eq`/`tabstop_copy`/`tabstop_count`/`tabstop_first` need NO
//! Rust equivalent at all: given this crate's own `Option<&[ColnrT]>`/
//! `Option<Vec<ColnrT>>` representation of `vts` (see below), plain
//! `==`/`.to_vec()`/`.clone()`/`.map_or(0, |v| v.len())`/
//! `.map_or(8, |v| v[0])` already do exactly what each one does by
//! hand - the same reasoning already established for `optval_free`/
//! `optval_copy`/`optval_equal` (`option.rs`).
//!
//! `preprocs_left` remains deferred - needs `indent_c.c`'s
//! `in_cinkeys` (real `'cinkeys'`/`'cinwords'` matching, not just a
//! fixed-default-rule shortcut).
//!
//! Also translated: `briopt_check` - parses `'breakindentopt'`'s value
//! (`shift:`/`min:`/`sbr`/`list:`/`column:`, comma-separated) into a
//! window's own `w_briopt_*` fields, needed only the already-real
//! `charset.rs`'s `getdigits`/`getdigits_int`. No real caller is
//! translated yet (`did_set_breakindentopt`, the option's own
//! callback) - harvested ahead of it, matching this crate's
//! established precedent for a small, self-contained function.
//!
//! `tabstop_padding`'s `vts` parameter deviates from the original's raw
//! `colnr_T *vts` (a C array whose own `vts[0]` holds the element
//! count, `vts[1..=count]` the actual tab-stop widths - a classic C
//! "self-describing array" idiom): here it's a plain slice of tab-stop
//! widths with no redundant leading count element, matching this
//! crate's usual "idiomatic Rust equivalent of the C resource, not its
//! exact bit representation" convention (the `Vec`'s own `.len()`
//! already provides the count). `buffer_defs.rs`'s `BufT.b_p_vts_array`/
//! `b_p_vsts_array` fields (`Option<Vec<ColnrT>>`, translated much
//! earlier, before anything used them for real) are read the same way
//! by this function, their first real consumer - established here as
//! the fields' own convention going forward, not just a one-off
//! choice for this call site. `tabstop_at`/`tabstop_start`/
//! `tabstop_fromto` all follow the exact same convention.
//!
//! Deferred: everything else in the file.

use crate::buffer_defs::{BufT, WinT};
use crate::globals::GlobalCell;
use crate::pos_defs::ColnrT;
use crate::types_defs::{HandleT, OptInt};

/// File-static cache for [`get_breakindent_win`] (the original's own
/// 10 `static` locals inside that function, bundled into one struct
/// here matching this crate's established `GlobalCell`-backed-
/// file-static convention, e.g. `buffer.rs`'s `TOP_FILE_NUM`).
///
/// `prev_vts` is compared by VALUE here (`Option<Vec<ColnrT>>`
/// equality), not by the original's raw pointer identity check
/// (`prev_vts != wp->w_buffer->b_p_vts_array`) - this crate's
/// `b_p_vts_array` is an owned `Vec` with no stable cross-buffer/
/// cross-mutation pointer identity to compare instead. This can only
/// ever invalidate the cache in cases where pointer-identity
/// comparison wouldn't have (never the reverse), which is safe for a
/// performance-only cache: it costs an occasional extra recompute, it
/// can never produce an incorrect cached value.
#[derive(Default)]
struct BreakindentCache {
    /// cached indent value (`prev_indent`)
    prev_indent: i32,
    /// cached tabstop value (`prev_ts`)
    prev_ts: OptInt,
    /// cached vartabs values (`prev_vts`) - see this struct's own doc
    /// comment for how this differs from the original's pointer check.
    prev_vts: Option<Vec<ColnrT>>,
    /// cached buffer number (`prev_fnum`)
    prev_fnum: HandleT,
    /// cached copy of "line" (`prev_line`)
    prev_line: Vec<u8>,
    /// changedtick of cached value (`prev_tick`)
    prev_tick: crate::eval::typval_defs::VarnumberT,
    /// cached list indent (`prev_list`)
    prev_list: i32,
    /// cached `w_p_briopt_list` value (`prev_listopt`)
    prev_listopt: i32,
    /// cached `no_ts` value (`prev_no_ts`)
    prev_no_ts: bool,
    /// cached `'display'` `"uhex"` value (`prev_dy_uhex`)
    prev_dy_uhex: u32,
    /// cached `'formatlistpat'` value (`prev_flp`)
    prev_flp: Option<Vec<u8>>,
}

static BREAKINDENT_CACHE: std::sync::LazyLock<GlobalCell<BreakindentCache>> =
    std::sync::LazyLock::new(|| GlobalCell::new(BreakindentCache::default()));

/// Set the integer values corresponding to the string setting of
/// `'vartabstop'`/`'varsofttabstop'` (`tabstop_set`).
///
/// Returns `Ok(None)` for an empty (or literal `"0"`) `var` (matching
/// the "not set" sentinel these option values use), `Ok(Some(widths))`
/// on a successfully-parsed comma-separated list of positive
/// integers, or `Err(())` on invalid syntax/an out-of-range value -
/// the original's own `emsg`/`semsg` message display is skipped
/// (`message.c`'s display pipeline is not tractable), matching this
/// crate's established policy elsewhere; only the success/failure
/// OUTCOME is preserved. Callers pass `Option<Vec<u8>>` option fields
/// via `.as_deref().unwrap_or(&[])` (this crate's own established
/// "no trailing NUL for option string values" convention, `option.rs`'s
/// module doc), matching `var[0] == NUL` for both an absent and an
/// empty value.
// The `Err` case genuinely has no payload to carry (message display
// is skipped, matching this crate's own established policy) and the
// 3-way "unset"/"set"/"invalid" distinction (mirroring the original's
// `bool` return + `colnr_T **` out-param exactly) doesn't collapse
// cleanly into a plain `Option<Vec<ColnrT>>` the way `Option<Callback>`
// could for `callback_from_typval` (which had only 2 real outcomes) -
// a dedicated marker error type would carry no more information than
// `()` already does.
/// Whether `'indentexpr'` should be used for Lisp indenting
/// (`use_indentexpr_for_lisp`).
///
/// All three conditions must hold: the buffer is in Lisp mode,
/// `'indentexpr'` is non-empty, and `'lispoptions'` is exactly
/// `"expr:1"`. Callers may also want to check `'autoindent'`.
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer
/// to a live `BufT`.
#[must_use]
pub unsafe fn use_indentexpr_for_lisp() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &*crate::globals::GLOBALS.get_mut().curbuf };
    curbuf.b_p_lisp != 0
        && curbuf.b_p_inde.as_deref().is_some_and(|inde| !inde.is_empty())
        && curbuf.b_p_lop.as_deref() == Some(b"expr:1")
}

#[allow(clippy::result_unit_err)]
pub fn tabstop_set(var: &[u8]) -> Result<Option<Vec<ColnrT>>, ()> {
    if var.is_empty() || var == b"0" {
        return Ok(None);
    }

    // First pass: validate syntax (digits and properly-placed commas
    // only), counting how many comma-separated values there are.
    let mut valcount = 1usize;
    for i in 0..var.len() {
        if i == 0 || var[i - 1] == b',' {
            // Use def=1 so overflow/too-large values pass this check
            // and are instead rejected by the "n > TABSTOP_MAX" check
            // in the second pass below.
            let (value, _) = crate::charset::getdigits(&var[i..], false, 1);
            if value <= 0 {
                return Err(());
            }
        }

        if crate::ascii_defs::ascii_isdigit(i32::from(var[i])) {
            continue;
        }
        if var[i] == b',' && i > 0 && var[i - 1] != b',' && i + 1 < var.len() {
            valcount += 1;
            continue;
        }
        return Err(());
    }

    // Second pass: actually parse each comma-separated value. Every
    // "start of number" position was already validated above (only
    // digits, no sign), so `getdigits` here can only ever produce a
    // genuine non-negative value or (for an astronomically long
    // digit run) a graceful `def=0` fallback - never the original's
    // own `atoi`-on-overflow undefined behavior.
    let mut array = vec![0 as ColnrT; valcount];
    let mut t = 0usize;
    let mut cp = 0usize;
    while cp < var.len() {
        let (n, _) = crate::charset::getdigits(&var[cp..], false, 0);
        if n <= 0 || n > i64::from(crate::option_vars::TABSTOP_MAX) {
            return Err(());
        }
        array[t] = n as ColnrT;
        t += 1;
        while cp < var.len() && var[cp] != b',' {
            cp += 1;
        }
        if cp < var.len() {
            cp += 1;
        }
    }

    Ok(Some(array))
}

/// Return a count of the number of tabstops (`tabstop_count`).
///
/// See this module's own doc comment for how `ts` differs from the
/// original's raw, self-counting `colnr_T *` array: the count lives in
/// the slice's own length rather than in a leading element.
#[must_use]
pub fn tabstop_count(ts: Option<&[ColnrT]>) -> i32 {
    ts.map_or(0, |t| t.len() as i32)
}

/// Return the first tabstop, or `8` if there are no tabstops defined
/// (`tabstop_first`).
///
/// See [`tabstop_count`] for the representation note.
#[must_use]
pub fn tabstop_first(ts: Option<&[ColnrT]>) -> i32 {
    ts.and_then(|t| t.first().copied()).unwrap_or(8)
}

/// Whether two tabstop arrays hold the same widths (`tabstop_eq`).
///
/// See [`tabstop_count`] for the representation note. The original
/// compares the leading count element and then each width in turn;
/// with the count carried by the slice itself, that is exactly slice
/// equality.
#[must_use]
pub fn tabstop_eq(ts1: Option<&[ColnrT]>, ts2: Option<&[ColnrT]>) -> bool {
    ts1 == ts2
}

/// Copy a tabstop array (`tabstop_copy`).
///
/// See [`tabstop_count`] for the representation note. The original
/// hands back an `xmalloc`ed array the caller must free; an owned
/// `Vec` carries the same ownership without the manual free.
#[must_use]
pub fn tabstop_copy(oldts: Option<&[ColnrT]>) -> Option<Vec<ColnrT>> {
    oldts.map(<[ColnrT]>::to_vec)
}

/// Flags for [`set_indent`] (`indent.h`'s own anonymous `enum`).
pub mod sin_flag {
    /// Call `changed_bytes()` when the line changed (`SIN_CHANGED`).
    pub const CHANGED: i32 = 1;
    /// Insert the indent before the existing text (`SIN_INSERT`).
    pub const INSERT: i32 = 2;
    /// Save the line for undo before changing it (`SIN_UNDO`).
    pub const UNDO: i32 = 4;
    /// Don't adjust extmarks (`SIN_NOMARK`).
    pub const NOMARK: i32 = 8;
}

/// Set the indent of the current line to `size` spaces' worth
/// (`set_indent`).
///
/// `size` is measured in spaces; the actual characters used honour
/// `'expandtab'`, `'preserveindent'`, `'tabstop'` and `'vartabstop'`.
/// See [`sin_flag`] for `flags`.
///
/// Returns `true` when the line was actually changed.
///
/// # Adaptation
///
/// The original walks the line with a `char *p` and builds the
/// replacement through a second `char *s` into one `xmalloc`ed
/// buffer. Here `get_cursor_line_ptr` hands back an owned copy, so
/// the walk uses an index into it and the replacement is built into a
/// `Vec`. The two pointer differences the tail needs - `p - oldline`
/// and `s - newline` - become that index and the `Vec`'s length taken
/// before the tail is appended.
///
/// # Safety
/// `GLOBALS.curwin`/`curbuf` must be valid, with a live memline.
pub unsafe fn set_indent(size: i32, flags: i32) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    let curwin = g.curwin;
    let curbuf = g.curbuf;
    // SAFETY: forwarded from this function's own safety doc.
    let (et, pi, ts, vts) = {
        let b = unsafe { &*curbuf };
        (b.b_p_et, b.b_p_pi, b.b_p_ts, b.b_p_vts_array.clone())
    };
    let vts = vts.as_deref();

    let mut doit = false;
    let mut ind_done = 0; // Measured in spaces.
    let mut tab_pad;
    let mut retval = false;

    // Number of initial whitespace chars when 'et' and 'pi' are both set.
    let mut orig_char_len: i32 = -1;

    // First check if there is anything to do, and compute the number
    // of characters needed for the indent.
    let mut todo = size;
    let mut ind_len = 0; // Measured in characters.
    // SAFETY: forwarded from this function's own safety doc.
    let oldline = unsafe { crate::cursor::get_cursor_line_ptr() };
    let mut p = 0usize;
    // SAFETY: forwarded from this function's own safety doc.
    let mut line_len = unsafe { crate::cursor::get_cursor_line_len() } + 1;

    let at = |i: usize| oldline.get(i).copied().unwrap_or(0);

    // Calculate the buffer size for the new indent, and check whether
    // it isn't already set. If 'expandtab' isn't set use TABs; if both
    // 'expandtab' and 'preserveindent' are set, count the number of
    // characters at the beginning of the line to be copied.
    if et == 0 || (flags & sin_flag::INSERT == 0 && pi != 0) {
        let mut ind_col = 0;
        // If 'preserveindent' is set then reuse as much as possible of
        // the existing indent structure for the new indent.
        if flags & sin_flag::INSERT == 0 && pi != 0 {
            ind_done = 0;

            // Count as many characters as we can use.
            while todo > 0 && crate::ascii_defs::ascii_iswhite(i32::from(at(p))) {
                if at(p) == crate::ascii_defs::TAB {
                    tab_pad = tabstop_padding(ind_done, ts, vts);
                    // Stop if this tab will overshoot the target.
                    if todo < tab_pad {
                        break;
                    }
                    todo -= tab_pad;
                    ind_len += 1;
                    ind_done += tab_pad;
                } else {
                    todo -= 1;
                    ind_len += 1;
                    ind_done += 1;
                }
                p += 1;
            }

            // These diverge from this point.
            ind_col = ind_done;
            // Set the initial number of whitespace chars to copy if we
            // are preserving indent but 'expandtab' is set.
            if et != 0 {
                orig_char_len = ind_len;
            }
            // Fill to the next tabstop with a tab, if possible.
            tab_pad = tabstop_padding(ind_done, ts, vts);
            if todo >= tab_pad && orig_char_len == -1 {
                doit = true;
                todo -= tab_pad;
                ind_len += 1;
                ind_col += tab_pad;
            }
        }

        // Count the tabs required for the indent.
        loop {
            tab_pad = tabstop_padding(ind_col, ts, vts);
            if todo < tab_pad {
                break;
            }
            if at(p) == crate::ascii_defs::TAB {
                p += 1;
            } else {
                doit = true;
            }
            todo -= tab_pad;
            ind_len += 1;
            ind_col += tab_pad;
        }
    }

    // Count the spaces required for the indent.
    while todo > 0 {
        if at(p) == b' ' {
            p += 1;
        } else {
            doit = true;
        }
        todo -= 1;
        ind_len += 1;
    }

    // Return if the indent is OK already.
    if !doit && !crate::ascii_defs::ascii_iswhite(i32::from(at(p))) && flags & sin_flag::INSERT == 0 {
        return false;
    }

    // Work out where the preserved text starts.
    if flags & sin_flag::INSERT != 0 {
        p = 0;
    } else {
        p += crate::charset::skipwhite(&oldline[p.min(oldline.len())..]);
        line_len -= p as ColnrT;
    }

    let mut newline: Vec<u8> = Vec::new();
    // Number of columns (in bytes) that were preserved.
    let mut skipcols = 0;
    if orig_char_len != -1 {
        // If 'preserveindent' and 'expandtab' are both set, keep the
        // original characters; the rest is filled with spaces below.
        todo = size - ind_done;
        // Set the total length of the indent in characters, which may
        // have been undercounted until now.
        ind_len = orig_char_len + todo;
        p = 0;
        skipcols = orig_char_len;

        for _ in 0..orig_char_len {
            newline.push(at(p));
            p += 1;
        }

        // Skip over any additional white space (useful when the new
        // indent is less than the old one).
        while crate::ascii_defs::ascii_iswhite(i32::from(at(p))) {
            p += 1;
        }
    } else {
        todo = size;
    }

    // Put the characters in the new line. If 'expandtab' isn't set,
    // use TABs.
    if et == 0 {
        // If 'preserveindent' is set then reuse as much as possible of
        // the existing indent structure for the new indent.
        if flags & sin_flag::INSERT == 0 && pi != 0 {
            p = 0;
            ind_done = 0;

            while todo > 0 && crate::ascii_defs::ascii_iswhite(i32::from(at(p))) {
                if at(p) == crate::ascii_defs::TAB {
                    tab_pad = tabstop_padding(ind_done, ts, vts);
                    // Stop if this tab will overshoot the target.
                    if todo < tab_pad {
                        break;
                    }
                    todo -= tab_pad;
                    ind_done += tab_pad;
                } else {
                    todo -= 1;
                    ind_done += 1;
                }
                newline.push(at(p));
                p += 1;
                skipcols += 1;
            }

            // Fill to the next tabstop with a tab, if possible.
            tab_pad = tabstop_padding(ind_done, ts, vts);
            if todo >= tab_pad {
                newline.push(crate::ascii_defs::TAB);
                todo -= tab_pad;
                ind_done += tab_pad;
            }
            p += crate::charset::skipwhite(&oldline[p.min(oldline.len())..]);
        }

        loop {
            tab_pad = tabstop_padding(ind_done, ts, vts);
            if todo < tab_pad {
                break;
            }
            newline.push(crate::ascii_defs::TAB);
            todo -= tab_pad;
            ind_done += tab_pad;
        }
    }

    while todo > 0 {
        newline.push(b' ');
        todo -= 1;
    }

    // `s - newline` in the original: the length before the tail is
    // appended.
    let new_offset = newline.len() as ColnrT;
    let old_offset = p as ColnrT;
    let tail_end = (p + line_len.max(0) as usize).min(oldline.len());
    newline.extend_from_slice(&oldline[p.min(oldline.len())..tail_end]);

    // SAFETY: forwarded from this function's own safety doc.
    let lnum = unsafe { &*curwin }.w_cursor.lnum;
    // Replace the line (unless undo fails).
    // SAFETY: forwarded from this function's own safety doc.
    if flags & sin_flag::UNDO == 0
        || unsafe { crate::undo::u_savesub(lnum) } == crate::vim_defs::OK
    {
        // SAFETY: forwarded from this function's own safety doc.
        let _ = unsafe { crate::memline::ml_replace(lnum, &newline) };
        if flags & sin_flag::NOMARK == 0 {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe {
                crate::extmark::extmark_splice_cols(
                    &mut *curbuf,
                    lnum - 1,
                    skipcols,
                    old_offset - skipcols,
                    new_offset - skipcols,
                    crate::extmark_defs::ExtmarkOp::Undo,
                );
            }
        }

        if flags & sin_flag::CHANGED != 0 {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::change::changed_bytes(lnum, 0) };
        }

        // Correct the saved cursor position if it is in this line.
        // SAFETY: forwarded from this function's own safety doc.
        let saved = unsafe { crate::globals::GLOBALS.get_mut() };
        if saved.saved_cursor.lnum == lnum {
            if saved.saved_cursor.col >= old_offset {
                // The cursor was after the indent, so adjust for the
                // number of bytes added or removed.
                saved.saved_cursor.col += ind_len - old_offset;
            } else if saved.saved_cursor.col >= new_offset {
                // The cursor was in the indent and is now after it, so
                // put it back at the start of the indent (replacing
                // spaces with a TAB).
                saved.saved_cursor.col = new_offset;
            }
        }
        retval = true;
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { &mut *curwin }.w_cursor.col = ind_len;
    retval
}

/// Whether the word at `p` is one of the `'lispwords'`
/// (`lisp_match`).
///
/// The buffer-local `'lispwords'` takes precedence over the global one
/// when it is non-empty. A word matches only when the text at `p`
/// starts with it AND the next character is whitespace or the end of
/// the line, so a longer identifier with the same prefix does not
/// match.
///
/// # Safety
/// Same as [`may_do_si`].
#[must_use]
pub unsafe fn lisp_match(p: &[u8]) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &*crate::globals::GLOBALS.get_mut().curbuf };
    let local = curbuf.b_p_lw.clone().unwrap_or_default();
    let words = if local.is_empty() {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }
            .p_lispwords
            .clone()
            .unwrap_or_default()
    } else {
        local
    };

    // The original's `buf[512]` is a fixed scratch buffer; here
    // `copy_option_part` hands back an owned part instead, so the same
    // cap is passed through for faithfulness rather than necessity.
    let mut idx = 0usize;
    while idx < words.len() && words[idx] != 0 {
        let (part, next) = crate::option::copy_option_part(&words, idx, 512, b",");
        idx = next;
        let len = part.len();
        if p.len() >= len
            && p[..len] == part[..]
            && crate::ascii_defs::ascii_iswhite_or_nul(i32::from(p.get(len).copied().unwrap_or(0)))
        {
            return true;
        }
    }
    false
}

/// Whether preprocessor lines are left alone when indenting
/// (`preprocs_left`).
///
/// # Scope
///
/// The guard is real and translated in full, and resolves without
/// `in_cinkeys` for every buffer this crate can build: `'cindent'`
/// (`b_p_cin`) is `0` by default, so `&&` short-circuits the second
/// operand before `in_cinkeys` is reached. That branch is
/// `unimplemented!()`, needing `indent_c.c`'s own `in_cinkeys`.
///
/// # Safety
/// Same as [`may_do_si`].
#[must_use]
pub unsafe fn preprocs_left() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &*crate::globals::GLOBALS.get_mut().curbuf };
    if curbuf.b_p_si != 0 && curbuf.b_p_cin == 0 {
        return true;
    }
    if curbuf.b_p_cin == 0 {
        return false;
    }
    unimplemented!(
        "the 'cindent' half needs indent_c.c's in_cinkeys, not yet translated; \
         unreachable while 'cindent' is off"
    );
}

/// Calculate the number of screen spaces a tab will occupy. If `vts`
/// is set then the tab widths are taken from that slice, otherwise
/// the value of `ts_arg` is used (`tabstop_padding`).
///
/// See this module's own doc comment for how `vts` differs from the
/// original's raw, self-counting `colnr_T *` array.
#[must_use]
pub fn tabstop_padding(col: ColnrT, ts_arg: OptInt, vts: Option<&[ColnrT]>) -> i32 {
    let ts: i64 = if ts_arg == 0 { 8 } else { ts_arg };

    let Some(vts) = vts.filter(|v| !v.is_empty()) else {
        return (ts - (i64::from(col) % ts)) as i32;
    };

    let mut tabcol: i64 = 0;
    let mut found = false;
    let mut padding = 0i32;
    for &width in vts {
        tabcol += i64::from(width);
        if tabcol > i64::from(col) {
            padding = (tabcol - i64::from(col)) as i32;
            found = true;
            break;
        }
    }
    if !found {
        // SAFETY-free: `vts` was already checked non-empty above, so
        // `.last()` always succeeds.
        let last = i64::from(*vts.last().unwrap());
        padding = (last - ((i64::from(col) - tabcol) % last)) as i32;
    }

    padding
}

/// Find the size of the tab interval that covers column `col`
/// (`tabstop_at`).
///
/// If this is being called as part of a shift operation, `col` is not
/// the cursor column but the column number to the left of the first
/// non-whitespace character in the line. If the shift is to the left
/// (`left == true`), returns the size of the tab interval to the left
/// of `col` instead of covering it.
///
/// See this module's own doc comment for how `vts` differs from the
/// original's raw, self-counting `colnr_T *` array.
#[must_use]
pub fn tabstop_at(col: ColnrT, ts: OptInt, vts: Option<&[ColnrT]>, left: bool) -> i32 {
    let Some(vts) = vts.filter(|v| !v.is_empty()) else {
        return ts as i32;
    };

    let tabcount = vts.len();
    let mut tabcol: i64 = 0;
    let mut tab_size = 0i32;
    let mut t = 1usize;
    let mut matched = false;
    while t <= tabcount {
        tabcol += i64::from(vts[t - 1]);
        if tabcol > i64::from(col) {
            if left && t == 1 {
                tab_size = col;
            } else {
                let idx = if left { t - 1 } else { t };
                tab_size = vts[idx - 1];
            }
            matched = true;
            break;
        }
        t += 1;
    }
    if !matched {
        tab_size = vts[tabcount - 1];
    }

    tab_size
}

/// Find the column on which a tab starts (`tabstop_start`).
///
/// See this module's own doc comment for how `vts` differs from the
/// original's raw, self-counting `colnr_T *` array.
#[must_use]
pub fn tabstop_start(col: ColnrT, ts: i32, vts: Option<&[ColnrT]>) -> i32 {
    let Some(vts) = vts.filter(|v| !v.is_empty()) else {
        return col - col % ts;
    };

    let tabcount = vts.len();
    let mut tabcol: i64 = 0;
    for t in 1..=tabcount {
        tabcol += i64::from(vts[t - 1]);
        if tabcol > i64::from(col) {
            return (tabcol - i64::from(vts[t - 1])) as i32;
        }
    }

    let last = i64::from(vts[tabcount - 1]);
    let excess = tabcol % last;
    (i64::from(col) - (i64::from(col) - excess) % last) as i32
}

/// Find the number of tabs and spaces necessary to get from column
/// `start_col` to `end_col` (`tabstop_fromto`).
///
/// See this module's own doc comment for how `vts` differs from the
/// original's raw, self-counting `colnr_T *` array. Returns
/// `(ntabs, nspcs)` instead of writing through 2 `int *` out-params.
///
/// # Safety
/// If `ts_arg == 0`, `crate::globals::GLOBALS.curbuf` must be a
/// valid, non-null pointer to a live `BufT` (used to look up the
/// effective `'tabstop'` value, matching the original's own
/// `curbuf->b_p_ts` fallback).
pub unsafe fn tabstop_fromto(
    start_col: ColnrT,
    end_col: ColnrT,
    ts_arg: i32,
    vts: Option<&[ColnrT]>,
) -> (i32, i32) {
    let mut spaces = end_col - start_col;
    let ts: i64 = if ts_arg == 0 {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*crate::globals::GLOBALS.get_mut().curbuf).b_p_ts }
    } else {
        i64::from(ts_arg)
    };
    debug_assert!(ts != 0);

    let Some(vts) = vts.filter(|v| !v.is_empty()) else {
        let mut tabs = 0i32;
        let initspc = (ts - (i64::from(start_col) % ts)) as i32;
        if spaces >= initspc {
            spaces -= initspc;
            tabs += 1;
        }
        tabs += spaces / ts as i32;
        spaces -= (spaces / ts as i32) * ts as i32;
        return (tabs, spaces);
    };

    // Find the padding needed to reach the next tabstop.
    let tabcount = vts.len();
    let mut tabcol: i64 = 0;
    let mut t = 1usize;
    let mut found = false;
    while t <= tabcount {
        tabcol += i64::from(vts[t - 1]);
        if tabcol > i64::from(start_col) {
            found = true;
            break;
        }
        t += 1;
    }
    let mut padding: i32 = if found {
        (tabcol - i64::from(start_col)) as i32
    } else {
        let last = i64::from(vts[tabcount - 1]);
        (last - ((i64::from(start_col) - tabcol) % last)) as i32
    };

    // If the space needed is less than the padding no tabs can be used.
    if spaces < padding {
        return (0, spaces);
    }

    let mut ntabs = 1;
    spaces -= padding;

    // At least one tab has been used. See if any more will fit.
    loop {
        if spaces == 0 {
            break;
        }
        t += 1;
        if t > tabcount {
            break;
        }
        padding = vts[t - 1];
        if spaces < padding {
            return (ntabs, spaces);
        }
        ntabs += 1;
        spaces -= padding;
    }

    let last = i64::from(vts[tabcount - 1]);
    ntabs += (i64::from(spaces) / last) as i32;
    let nspcs = (i64::from(spaces) % last) as i32;
    (ntabs, nspcs)
}

/// Return the effective `'shiftwidth'` value for `buf`, using virtual
/// column `col` to select among `'vartabstop'` entries when
/// `'shiftwidth'` is zero (`get_sw_value_col`).
#[must_use]
pub fn get_sw_value_col(buf: &BufT, col: ColnrT, left: bool) -> i32 {
    if buf.b_p_sw != 0 {
        buf.b_p_sw as i32
    } else {
        tabstop_at(col, buf.b_p_ts, buf.b_p_vts_array.as_deref(), left)
    }
}

/// Return the effective `'shiftwidth'` value for `buf`, using the
/// 'tabstop' value when `'shiftwidth'` is zero (`get_sw_value`).
#[must_use]
pub fn get_sw_value(buf: &BufT) -> i32 {
    get_sw_value_col(buf, 0, false)
}

/// Idem, using `pos` (`get_sw_value_pos`).
///
/// # Safety
/// `buf` must be a valid, non-null pointer to a live `BufT`.
/// `crate::globals::GLOBALS.curwin` must be a valid, non-null pointer
/// to a live `WinT` - same requirement as
/// `crate::insert::get_nolist_virtcol`. `buf` is only dereferenced
/// AFTER `get_nolist_virtcol` returns (never held as a live reference
/// across that call), since a real call always has `buf` and
/// `GLOBALS.curbuf`/`curwin.w_buffer` pointing at the very same
/// buffer - holding a `&BufT` across `get_nolist_virtcol`'s own
/// internal `GLOBALS.curbuf`-based access would be a genuine
/// aliasing hazard.
unsafe fn get_sw_value_pos(buf: *mut BufT, pos: &crate::pos_defs::PosT, left: bool) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let curwin = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
    // SAFETY: forwarded from this function's own safety doc.
    let save_cursor = unsafe { (*curwin).w_cursor };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        (*curwin).w_cursor = *pos;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let col = unsafe { crate::insert::get_nolist_virtcol() };
    // SAFETY: forwarded from this function's own safety doc - `buf`
    // is dereferenced fresh here, only after the call above.
    let sw_value = get_sw_value_col(unsafe { &*buf }, col, left);
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        (*curwin).w_cursor = save_cursor;
    }
    sw_value
}

/// Idem, using the first non-blank in the current line
/// (`get_sw_value_indent`).
///
/// # Safety
/// `buf` must be a valid, non-null pointer to a live `BufT`.
/// `crate::globals::GLOBALS.curwin` must be a valid, non-null pointer
/// to a live `WinT` with a valid `w_buffer` (forwarded from
/// `crate::charset::getwhitecols_curline`'s own safety doc, plus
/// `get_sw_value_pos`'s own).
#[must_use]
pub unsafe fn get_sw_value_indent(buf: *mut BufT, left: bool) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let mut pos = unsafe { (*crate::globals::GLOBALS.get_mut().curwin).w_cursor };
    // SAFETY: forwarded from this function's own safety doc.
    pos.col = unsafe { crate::charset::getwhitecols_curline() } as ColnrT;
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { get_sw_value_pos(buf, &pos, left) }
}

/// Return the effective `'softtabstop'` value for the current buffer,
/// using the `'shiftwidth'` value when `'softtabstop'` is negative
/// (`get_sts_value`).
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer
/// to a live `BufT`.
#[must_use]
pub unsafe fn get_sts_value() -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &*crate::globals::GLOBALS.get_mut().curbuf };
    if curbuf.b_p_sts < 0 {
        get_sw_value(curbuf)
    } else {
        curbuf.b_p_sts as i32
    }
}

/// Return `true` if the cursor is before (or, with `extra == 0`, on)
/// the first non-blank in the current line (`inindent`).
///
/// Translated ahead of its own real callers (`insert.c`/`ops.c`/
/// `textobject.c`/`insexpand.c`, none translated), matching this
/// crate's established "small, self-contained, no design freedom to
/// get wrong" precedent.
///
/// # Safety
/// `crate::globals::GLOBALS.curwin`/`curbuf` must be valid, non-null
/// pointers to live `WinT`/`BufT` (forwarded from
/// `crate::cursor::get_cursor_line_ptr`'s own safety doc).
#[must_use]
pub unsafe fn inindent(extra: ColnrT) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let line = unsafe { crate::cursor::get_cursor_line_ptr() };
    let mut col: ColnrT = 0;
    for &c in &line {
        if !crate::ascii_defs::ascii_iswhite(i32::from(c)) {
            break;
        }
        col += 1;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let cursor_col = unsafe { (*crate::globals::GLOBALS.get_mut().curwin).w_cursor.col };
    col >= cursor_col + extra
}

/// Return `true` if the conditions are OK for smart indenting
/// (`may_do_si`).
///
/// Translated ahead of its own real callers (`insert.c`/`ops.c`,
/// neither translated) - same precedent as [`inindent`] above.
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer
/// to a live `BufT`.
#[must_use]
pub unsafe fn may_do_si() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &*crate::globals::GLOBALS.get_mut().curbuf };
    curbuf.b_p_si != 0
        && curbuf.b_p_cin == 0
        && curbuf.b_p_inde.as_deref().is_none_or(<[u8]>::is_empty)
        // SAFETY: forwarded from this function's own safety doc.
        && unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_paste == 0
}

/// Parse `'breakindentopt'`'s value (`shift:`/`min:`/`sbr`/`list:`/
/// `column:`, comma-separated) into `wp`'s own `w_briopt_*` fields
/// (`briopt_check`).
///
/// `briopt`, when `Some`, overrides reading `wp`'s own
/// `w_onebuf_opt.wo_briopt` value directly - matching the original's
/// own "use `briopt` if given, else fall back to `wp->w_p_briopt`,
/// else the empty string" 3-way fallback (used by
/// `did_set_breakindentopt` to validate a CANDIDATE value before it
/// is actually stored).
///
/// Returns `false` if the value contains an unrecognized entry (a
/// real parse failure) - `wp`'s own fields are only updated on
/// success, and only when `wp` is `Some`.
///
/// Its real caller `optionstr.rs`'s `did_set_breakindentopt` IS now
/// translated (it passes `Some(wp)` only when the value being set is
/// the window-local `'breakindentopt'` storage, matching the
/// original's own `varp == &win->w_p_briopt ? win : NULL` check).
#[must_use]
pub fn briopt_check(briopt: Option<&[u8]>, wp: Option<&mut WinT>) -> bool {
    let mut bri_shift = 0i32;
    let mut bri_min = 20i32;
    let mut bri_sbr = false;
    let mut bri_list = 0i32;
    let mut bri_vcol = 0i32;

    let owned;
    let p: &[u8] = match briopt {
        Some(b) => b,
        None => match wp.as_deref() {
            Some(w) => {
                owned = w.w_onebuf_opt.wo_briopt.clone().unwrap_or_default();
                &owned
            }
            None => crate::option_vars::EMPTY_STRING_OPTION,
        },
    };

    let mut pos = 0usize;
    loop {
        if p.get(pos).copied().unwrap_or(0) == 0 {
            break;
        }
        let rest = &p[pos..];
        if rest.starts_with(b"shift:")
            && ((rest.get(6).copied() == Some(b'-')
                && rest.get(7).is_some_and(|&c| crate::ascii_defs::ascii_isdigit(i32::from(c))))
                || rest.get(6).is_some_and(|&c| crate::ascii_defs::ascii_isdigit(i32::from(c))))
        {
            pos += 6;
            let (value, consumed) = crate::charset::getdigits_int(&p[pos..], true, 0);
            bri_shift = value;
            pos += consumed;
        } else if rest.starts_with(b"min:")
            && rest.get(4).is_some_and(|&c| crate::ascii_defs::ascii_isdigit(i32::from(c)))
        {
            pos += 4;
            let (value, consumed) = crate::charset::getdigits_int(&p[pos..], true, 0);
            bri_min = value;
            pos += consumed;
        } else if rest.starts_with(b"sbr") {
            pos += 3;
            bri_sbr = true;
        } else if rest.starts_with(b"list:") {
            pos += 5;
            let (value, consumed) = crate::charset::getdigits(&p[pos..], false, 0);
            bri_list = value as i32;
            pos += consumed;
        } else if rest.starts_with(b"column:") {
            pos += 7;
            let (value, consumed) = crate::charset::getdigits(&p[pos..], false, 0);
            bri_vcol = value as i32;
            pos += consumed;
        }

        let c = p.get(pos).copied().unwrap_or(0);
        if c != b',' && c != 0 {
            return false;
        }
        if c == b',' {
            pos += 1;
        }
    }

    let Some(wp) = wp else {
        return true;
    };

    wp.w_briopt_shift = bri_shift;
    wp.w_briopt_min = bri_min;
    wp.w_briopt_sbr = bri_sbr;
    wp.w_briopt_list = bri_list;
    wp.w_briopt_vcol = bri_vcol;

    true
}

/// Compute the size of the indent (in window cells) in `ptr`, without
/// tabstops (count a tab as `^I`/`<09>`) (`indent_size_no_ts`).
///
/// Assumes `ptr` is a well-formed line (this crate's own convention:
/// includes its own trailing NUL) - the original relies on always
/// eventually hitting a real NUL terminator to stop; running out of a
/// malformed, non-NUL-terminated slice is treated the same way here
/// (returns the accumulated `vcol` instead of panicking), matching
/// `mbyte.c`'s established "ran out of slice = terminator" precedent.
///
/// # Safety
/// Touches `crate::option_vars::OPTION_VARS` (via
/// [`crate::charset::byte2cells`]).
#[must_use]
pub unsafe fn indent_size_no_ts(ptr: &[u8]) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let tab_size = unsafe { crate::charset::byte2cells(i32::from(crate::ascii_defs::TAB)) };
    let mut vcol = 0;
    for &c in ptr {
        if c == b' ' {
            vcol += 1;
        } else if c == crate::ascii_defs::TAB {
            vcol += tab_size;
        } else {
            return vcol;
        }
    }
    vcol
}

/// Compute the size of the indent (in window cells) in `ptr`, using
/// tabstops (`indent_size_ts`).
///
/// The original also asserts `char2cells(' ') == 1` up front as an
/// internal sanity check - always true given this crate's own
/// [`crate::charset::char2cells`] (a space is unconditionally 1 cell,
/// independent of any option state), so it's omitted entirely here
/// rather than forcing this otherwise-safe, option-state-independent
/// function to become `unsafe` merely to re-verify something that can
/// never actually fail (matches the `CHECK()`-macro-is-a-no-op
/// precedent from `memline.rs`'s `ml_find_line`).
///
/// See this module's own doc comment for how `vts` differs from the
/// original's raw, self-counting `colnr_T *` array. Same
/// ran-out-of-slice handling as [`indent_size_no_ts`].
#[must_use]
pub fn indent_size_ts(ptr: &[u8], ts: OptInt, vts: Option<&[ColnrT]>) -> i32 {
    let mut vcol: i32 = 0;
    let mut pos = 0usize;
    let tabstop_width: i32;
    let mut next_tab_vcol: i32;

    match vts.filter(|v| !v.is_empty()) {
        None => {
            // tab has fixed width
            tabstop_width = if ts == 0 { 8 } else { ts as i32 };
            next_tab_vcol = tabstop_width;
        }
        Some(widths) => {
            // tab has variable width
            for &width in widths {
                let cur_vcol_before = vcol;
                vcol += width;
                debug_assert!(cur_vcol_before < vcol);

                let mut cur_vcol = cur_vcol_before;
                loop {
                    let Some(&c) = ptr.get(pos) else {
                        return cur_vcol;
                    };
                    pos += 1;
                    if c == b' ' {
                        cur_vcol += 1;
                    } else if c == crate::ascii_defs::TAB {
                        break;
                    } else {
                        return cur_vcol;
                    }
                    if cur_vcol == vcol {
                        break;
                    }
                }
            }

            tabstop_width = *widths.last().unwrap();
            next_tab_vcol = vcol + tabstop_width;
        }
    }

    debug_assert_ne!(tabstop_width, 0);
    loop {
        let Some(&c) = ptr.get(pos) else {
            return vcol;
        };
        pos += 1;
        if c == b' ' {
            vcol += 1;
            if vcol == next_tab_vcol {
                next_tab_vcol += tabstop_width;
            }
        } else if c == crate::ascii_defs::TAB {
            vcol = next_tab_vcol;
            next_tab_vcol += tabstop_width;
        } else {
            return vcol;
        }
    }
}

/// Count the size (in window cells) of the indent in the current
/// line (`get_indent`).
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf`/`curwin` must be valid, non-null
/// pointers to live `BufT`/`WinT` (forwarded from
/// `crate::cursor::get_cursor_line_ptr`'s own safety doc).
#[must_use]
pub unsafe fn get_indent() -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let line = unsafe { crate::cursor::get_cursor_line_ptr() };
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &*crate::globals::GLOBALS.get_mut().curbuf };
    indent_size_ts(&line, curbuf.b_p_ts, curbuf.b_p_vts_array.as_deref())
}

/// Count the size (in window cells) of the indent in line `lnum` of
/// the current buffer (`get_indent_lnum`).
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer
/// to a live `BufT` (forwarded from `crate::memline::ml_get`'s own
/// safety doc).
#[must_use]
pub unsafe fn get_indent_lnum(lnum: crate::pos_defs::LinenrT) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let line = unsafe { crate::memline::ml_get(lnum) };
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &*crate::globals::GLOBALS.get_mut().curbuf };
    indent_size_ts(&line, curbuf.b_p_ts, curbuf.b_p_vts_array.as_deref())
}

/// Count the size (in window cells) of the indent in line `lnum` of
/// buffer `buf` (`get_indent_buf`).
///
/// # Safety
/// Same as `crate::memline::ml_get_buf`'s own safety doc.
#[must_use]
pub unsafe fn get_indent_buf(buf: &mut crate::buffer_defs::BufT, lnum: crate::pos_defs::LinenrT) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let line = unsafe { crate::memline::ml_get_buf(buf, lnum) };
    indent_size_ts(&line, buf.b_p_ts, buf.b_p_vts_array.as_deref())
}

/// `"indent({lnum})"` function (`f_indent`).
///
/// # Safety
/// `GLOBALS.curbuf` must point to a valid, live `BufT`.
pub unsafe fn f_indent(argvars: &[crate::eval::typval_defs::TypvalT], rettv: &mut crate::eval::typval_defs::TypvalT) {
    use crate::eval::typval_defs::TypvalValue;

    // SAFETY: forwarded from this function's own safety doc.
    let lnum = unsafe { crate::eval::typval::tv_get_lnum(&argvars[0]) };
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
    // SAFETY: forwarded from this function's own safety doc.
    let line_count = unsafe { (*curbuf).b_ml.ml_line_count };
    rettv.value = TypvalValue::Number(if lnum >= 1 && lnum <= line_count {
        // SAFETY: forwarded from this function's own safety doc.
        i64::from(unsafe { get_indent_lnum(lnum) })
    } else {
        -1
    });
}

/// `"shiftwidth([{col}])"` function (`f_shiftwidth`).
///
/// # Safety
/// `GLOBALS.curbuf` must point to a valid, live `BufT`.
pub unsafe fn f_shiftwidth(
    argvars: &[crate::eval::typval_defs::TypvalT],
    rettv: &mut crate::eval::typval_defs::TypvalT,
) {
    use crate::eval::typval_defs::TypvalValue;

    rettv.value = TypvalValue::Number(0);

    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &*crate::globals::GLOBALS.get_mut().curbuf };
    if !argvars.is_empty() {
        let col = crate::eval::typval::tv_get_number_chk(&argvars[0], None) as ColnrT;
        if col < 0 {
            // type error; errmsg already given (skipped here)
            return;
        }
        rettv.value = TypvalValue::Number(i64::from(get_sw_value_col(curbuf, col, false)));
        return;
    }
    rettv.value = TypvalValue::Number(i64::from(get_sw_value(curbuf)));
}

/// Return appropriate space number for `'breakindent'`, taking
/// influencing parameters into account (`get_breakindent_win`). `wp`
/// must be specified since it's not necessarily always the current
/// window.
///
/// # Deferred
/// The original also handles `'breakindentopt'` `"list"` (extra
/// indent for numbered lists, detected via `'formatlistpat'` regex
/// matching) when `w_briopt_list != 0 && w_briopt_vcol == 0` - this
/// needs the real regex engine (`regexp.c`'s `vim_regcomp`/
/// `vim_regexec`, not yet translated), a genuinely separate,
/// substantial subsystem. Rather than silently producing a wrong
/// indent value for this specific, discrete, opt-in configuration
/// (the caller must explicitly set `'breakindentopt'` to include
/// `"list"` - not a value reachable through ordinary use), this
/// `unimplemented!()`s there instead - matching `window.rs`'s
/// `win_fdccol_count` precedent for `'foldcolumn'=auto`. Every other
/// case (the common, default configuration) is fully correct.
///
/// # Safety
/// `wp.w_buffer` must be a valid, non-null pointer to a live `BufT`.
/// Touches the shared `BREAKINDENT_CACHE` global (file-static in the
/// original) and `crate::option_vars::OPTION_VARS` (via
/// `get_flp_value`/`get_showbreak_value`/`vim_strsize`, and
/// transitively via `win_col_off`/`win_col_off2`).
#[must_use]
pub unsafe fn get_breakindent_win(wp: &mut WinT, line: &[u8]) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let cache = unsafe { BREAKINDENT_CACHE.get_mut() };
    // SAFETY: forwarded from this function's own safety doc.
    let buf = unsafe { &*wp.w_buffer };

    // window width minus window margin space, i.e. what rests for text
    let eff_wwidth = wp.w_view_width
        // SAFETY: forwarded from this function's own safety doc.
        - unsafe { crate::r#move::win_col_off(wp) }
        // SAFETY: forwarded from this function's own safety doc.
        + unsafe { crate::r#move::win_col_off2(wp) };

    // In list mode, if 'listchars' "tab" isn't set, a TAB is displayed as ^I.
    let no_ts = wp.w_onebuf_opt.wo_list != 0 && wp.w_p_lcs_chars.tab1 == 0;

    // SAFETY: forwarded from this function's own safety doc.
    let dy_uhex = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.dy_flags
        & crate::option_vars::opt_dy_flag::UHEX;
    let flp = crate::option::get_flp_value(buf);

    // Used cached indent, unless
    // - buffer changed, or
    // - 'tabstop' changed, or
    // - 'vartabstop' changed, or
    // - buffer was changed, or
    // - 'breakindentopt' "list" changed, or
    // - 'list' or 'listchars' "tab" changed, or
    // - 'display' "uhex" flag changed, or
    // - 'formatlistpat' changed, or
    // - line changed.
    if cache.prev_fnum != buf.handle
        || cache.prev_ts != buf.b_p_ts
        || cache.prev_vts != buf.b_p_vts_array
        || cache.prev_tick != crate::buffer::buf_get_changedtick(buf)
        || cache.prev_listopt != wp.w_briopt_list
        || cache.prev_no_ts != no_ts
        || cache.prev_dy_uhex != dy_uhex
        || cache.prev_flp.as_deref() != Some(flp.as_slice())
        || cache.prev_line != line
    {
        cache.prev_fnum = buf.handle;
        cache.prev_line = line.to_vec();
        cache.prev_ts = buf.b_p_ts;
        cache.prev_vts.clone_from(&buf.b_p_vts_array);
        if wp.w_briopt_vcol == 0 {
            cache.prev_indent = if no_ts {
                // SAFETY: forwarded from this function's own safety doc.
                unsafe { indent_size_no_ts(line) }
            } else {
                indent_size_ts(line, buf.b_p_ts, buf.b_p_vts_array.as_deref())
            };
        }
        cache.prev_tick = crate::buffer::buf_get_changedtick(buf);
        cache.prev_listopt = wp.w_briopt_list;
        cache.prev_list = 0;
        cache.prev_no_ts = no_ts;
        cache.prev_dy_uhex = dy_uhex;
        cache.prev_flp = Some(flp);

        // add additional indent for numbered lists
        if wp.w_briopt_list != 0 && wp.w_briopt_vcol == 0 {
            unimplemented!(
                "'breakindentopt'=list needs regexp.c's real vim_regcomp/vim_regexec, not yet translated"
            );
        }
    }

    let mut bri;
    if wp.w_briopt_vcol != 0 {
        // column value has priority
        bri = wp.w_briopt_vcol;
        cache.prev_list = 0;
    } else {
        bri = cache.prev_indent + wp.w_briopt_shift;
    }

    // Add offset for number column, if 'n' is in 'cpoptions'
    // SAFETY: forwarded from this function's own safety doc.
    bri += unsafe { crate::r#move::win_col_off2(wp) };

    // add additional indent for numbered lists
    if wp.w_briopt_list > 0 {
        bri += cache.prev_list;
    }

    // indent minus the length of the showbreak string
    if wp.w_briopt_sbr {
        // SAFETY: forwarded from this function's own safety doc.
        bri -= unsafe { crate::charset::vim_strsize(&crate::option::get_showbreak_value(wp)) };
    }

    // never indent past left window margin
    if bri < 0 {
        bri = 0;
    } else if bri > eff_wwidth - wp.w_briopt_min {
        // always leave at least bri_min characters on the left,
        // if text width is sufficient
        bri = (eff_wwidth - wp.w_briopt_min).max(0);
    }

    bri
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer_defs::BufT;

    /// Builds a buffer/window pair for the [`set_indent`] tests.
    ///
    /// `set_indent` reaches `extmark_splice_cols`, which asks for an
    /// undo header; without a pre-existing one `u_force_get_undo_header`
    /// tries to create one, which needs undo state this fixture does
    /// not build. Installing a real header keeps the test exercising
    /// `set_indent` itself rather than the undo machinery.
    fn set_indent_fixture(et: i32, line: &[u8]) -> (Box<BufT>, Box<WinT>) {
        let mut buf = Box::new(BufT {
            b_p_et: et,
            b_p_ts: 8,
            b_u_curhead: Box::into_raw(Box::new(crate::undo_defs::UHeader::default())),
            ..Default::default()
        });
        assert_eq!(
            unsafe { crate::memline::ml_open(&mut buf) },
            crate::vim_defs::OK
        );
        let mut owned = line.to_vec();
        owned.push(0);
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(&mut buf, 1, &owned) },
            crate::vim_defs::OK
        );
        let win = Box::new(WinT {
            w_cursor: crate::pos_defs::PosT { lnum: 1, col: 0, coladd: 0 },
            ..Default::default()
        });
        (buf, win)
    }

    fn close_set_indent_fixture(mut buf: Box<BufT>) {
        unsafe {
            if !buf.b_u_curhead.is_null() {
                drop(Box::from_raw(buf.b_u_curhead));
                buf.b_u_curhead = std::ptr::null_mut();
            }
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn set_indent_uses_a_tab_when_expandtab_is_off() {
        // Cross-verified against real nvim: with noexpandtab and
        // ts=8, shifting "abc" right by 8 columns yields "\tabc"
        // (strlen 4) - one TAB rather than eight spaces.
        let (mut buf, mut win) = set_indent_fixture(0, b"abc");
        let buf_ptr: *mut BufT = &mut *buf;
        let win_ptr: *mut WinT = &mut *win;
        let _guard = CursorTestGuard::set(win_ptr, buf_ptr);

        assert!(unsafe { set_indent(8, 0) });
        assert_eq!(unsafe { crate::memline::ml_get(1) }, b"\tabc\0");

        drop(_guard);
        close_set_indent_fixture(buf);
    }

    #[test]
    fn set_indent_uses_spaces_when_expandtab_is_on() {
        // Cross-verified against real nvim: with expandtab and sw=4,
        // shifting "abc" right yields "    abc" (strlen 7).
        let (mut buf, mut win) = set_indent_fixture(1, b"abc");
        let buf_ptr: *mut BufT = &mut *buf;
        let win_ptr: *mut WinT = &mut *win;
        let _guard = CursorTestGuard::set(win_ptr, buf_ptr);

        assert!(unsafe { set_indent(4, 0) });
        assert_eq!(unsafe { crate::memline::ml_get(1) }, b"    abc\0");

        drop(_guard);
        close_set_indent_fixture(buf);
    }

    #[test]
    fn set_indent_replaces_an_existing_indent() {
        let (mut buf, mut win) = set_indent_fixture(1, b"      abc");
        let buf_ptr: *mut BufT = &mut *buf;
        let win_ptr: *mut WinT = &mut *win;
        let _guard = CursorTestGuard::set(win_ptr, buf_ptr);

        // Shrinking the indent from 6 spaces to 2.
        assert!(unsafe { set_indent(2, 0) });
        assert_eq!(unsafe { crate::memline::ml_get(1) }, b"  abc\0");

        drop(_guard);
        close_set_indent_fixture(buf);
    }

    #[test]
    fn set_indent_reports_false_when_the_indent_already_matches() {
        let (mut buf, mut win) = set_indent_fixture(1, b"    abc");
        let buf_ptr: *mut BufT = &mut *buf;
        let win_ptr: *mut WinT = &mut *win;
        let _guard = CursorTestGuard::set(win_ptr, buf_ptr);

        assert!(
            !unsafe { set_indent(4, 0) },
            "already 4 spaces, so nothing to do"
        );
        assert_eq!(unsafe { crate::memline::ml_get(1) }, b"    abc\0");

        drop(_guard);
        close_set_indent_fixture(buf);
    }

    #[test]
    fn set_indent_removes_the_indent_entirely_for_zero() {
        let (mut buf, mut win) = set_indent_fixture(1, b"\t  abc");
        let buf_ptr: *mut BufT = &mut *buf;
        let win_ptr: *mut WinT = &mut *win;
        let _guard = CursorTestGuard::set(win_ptr, buf_ptr);

        assert!(unsafe { set_indent(0, 0) });
        assert_eq!(unsafe { crate::memline::ml_get(1) }, b"abc\0");

        drop(_guard);
        close_set_indent_fixture(buf);
    }

    #[test]
    fn set_indent_with_sin_insert_prepends_to_the_existing_text() {
        // SIN_INSERT keeps whatever is already there and puts the new
        // indent in front of it, rather than replacing it.
        let (mut buf, mut win) = set_indent_fixture(1, b"  abc");
        let buf_ptr: *mut BufT = &mut *buf;
        let win_ptr: *mut WinT = &mut *win;
        let _guard = CursorTestGuard::set(win_ptr, buf_ptr);

        assert!(unsafe { set_indent(2, sin_flag::INSERT) });
        assert_eq!(unsafe { crate::memline::ml_get(1) }, b"    abc\0");

        drop(_guard);
        close_set_indent_fixture(buf);
    }

    #[test]
    fn set_indent_leaves_the_cursor_at_the_indent_width() {
        let (mut buf, mut win) = set_indent_fixture(1, b"abc");
        let buf_ptr: *mut BufT = &mut *buf;
        let win_ptr: *mut WinT = &mut *win;
        let _guard = CursorTestGuard::set(win_ptr, buf_ptr);

        assert!(unsafe { set_indent(3, 0) });
        assert_eq!(
            unsafe { (*win_ptr).w_cursor.col },
            3,
            "cursor moved past the new indent"
        );

        drop(_guard);
        close_set_indent_fixture(buf);
    }

    #[test]
    fn tabstop_count_reports_the_number_of_stops() {
        assert_eq!(tabstop_count(None), 0);
        assert_eq!(tabstop_count(Some(&[])), 0);
        assert_eq!(tabstop_count(Some(&[4, 8, 12])), 3);
    }

    #[test]
    fn tabstop_first_falls_back_to_eight() {
        // The original returns 8 when there are no tabstops at all,
        // matching 'tabstop's own default.
        assert_eq!(tabstop_first(None), 8);
        assert_eq!(tabstop_first(Some(&[])), 8);
        assert_eq!(tabstop_first(Some(&[4, 8, 12])), 4);
    }

    #[test]
    fn tabstop_eq_compares_widths() {
        assert!(tabstop_eq(None, None));
        assert!(tabstop_eq(Some(&[4, 8]), Some(&[4, 8])));
        // One set but not the other.
        assert!(!tabstop_eq(None, Some(&[4])));
        assert!(!tabstop_eq(Some(&[4]), None));
        // Same length, different widths.
        assert!(!tabstop_eq(Some(&[4, 8]), Some(&[4, 9])));
        // Different lengths - the original's own leading-count check.
        assert!(!tabstop_eq(Some(&[4, 8]), Some(&[4, 8, 12])));
    }

    #[test]
    fn tabstop_copy_produces_an_independent_array() {
        assert_eq!(tabstop_copy(None), None);
        let src = [4, 8, 12];
        let copy = tabstop_copy(Some(&src)).expect("Some in, Some out");
        assert_eq!(copy, vec![4, 8, 12]);
        // Owned, so mutating the copy cannot touch the original.
        let mut copy = copy;
        copy[0] = 99;
        assert_eq!(src[0], 4);
    }

    /// Installs `buf` as `curbuf`, restoring the previous one on drop.
    struct CurbufGuard {
        prev: *mut BufT,
    }

    impl CurbufGuard {
        fn set(buf: &mut BufT) -> Self {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            let prev = globals.curbuf;
            globals.curbuf = buf as *mut BufT;
            Self { prev }
        }
    }

    impl Drop for CurbufGuard {
        fn drop(&mut self) {
            unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = self.prev;
        }
    }

    #[test]
    fn use_indentexpr_for_lisp_needs_all_three_conditions() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT {
            b_p_lisp: 1,
            b_p_inde: Some(b"MyIndent()".to_vec()),
            b_p_lop: Some(b"expr:1".to_vec()),
            ..Default::default()
        };
        let _guard = CurbufGuard::set(&mut buf);
        assert!(unsafe { use_indentexpr_for_lisp() });
    }

    #[test]
    fn use_indentexpr_for_lisp_is_false_when_any_condition_fails() {
        let _lock = crate::globals::global_state_test_lock();

        // Not in Lisp mode.
        let mut buf = BufT {
            b_p_lisp: 0,
            b_p_inde: Some(b"MyIndent()".to_vec()),
            b_p_lop: Some(b"expr:1".to_vec()),
            ..Default::default()
        };
        {
            let _guard = CurbufGuard::set(&mut buf);
            assert!(!unsafe { use_indentexpr_for_lisp() });
        }

        // 'indentexpr' empty, and absent.
        for inde in [Some(Vec::new()), None] {
            let mut buf = BufT {
                b_p_lisp: 1,
                b_p_inde: inde,
                b_p_lop: Some(b"expr:1".to_vec()),
                ..Default::default()
            };
            let _guard = CurbufGuard::set(&mut buf);
            assert!(!unsafe { use_indentexpr_for_lisp() });
        }

        // 'lispoptions' must match exactly, not merely start with it.
        for lop in [Some(b"expr:0".to_vec()), Some(b"expr:10".to_vec()), None] {
            let mut buf = BufT {
                b_p_lisp: 1,
                b_p_inde: Some(b"MyIndent()".to_vec()),
                b_p_lop: lop,
                ..Default::default()
            };
            let _guard = CurbufGuard::set(&mut buf);
            assert!(!unsafe { use_indentexpr_for_lisp() });
        }
    }

    #[test]
    fn get_breakindent_win_plain_indent_no_options() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { handle: 101, b_p_ts: 8, ..Default::default() };
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        win.w_view_width = 80;
        let line = b"    text\0"; // 4 leading spaces, ts=8: indent=4
        assert_eq!(unsafe { get_breakindent_win(&mut win, line) }, 4);
    }

    #[test]
    fn get_breakindent_win_briopt_shift_adds_to_indent() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { handle: 102, b_p_ts: 8, ..Default::default() };
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        win.w_view_width = 80;
        win.w_briopt_shift = 3;
        let line = b"    text\0"; // indent=4
        assert_eq!(unsafe { get_breakindent_win(&mut win, line) }, 7); // 4 + 3
    }

    #[test]
    fn get_breakindent_win_briopt_vcol_overrides_indent_and_resets_list() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { handle: 103, b_p_ts: 8, ..Default::default() };
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        win.w_view_width = 80;
        win.w_briopt_vcol = 15;
        let line = b"    text\0"; // indent would be 4, but vcol has priority
        assert_eq!(unsafe { get_breakindent_win(&mut win, line) }, 15);
    }

    #[test]
    fn get_breakindent_win_caches_until_something_relevant_changes() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { handle: 104, b_p_ts: 8, ..Default::default() };
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        win.w_view_width = 80;
        let line = b"    text\0";
        assert_eq!(unsafe { get_breakindent_win(&mut win, line) }, 4);

        // Corrupt the cached indent directly (kept well under the
        // window-width clamp threshold so it isn't itself clamped
        // away) - a second call with EVERYTHING unchanged should
        // return this corrupted value via the cache, not recompute
        // the real one (4).
        unsafe { BREAKINDENT_CACHE.get_mut() }.prev_indent = 50;
        assert_eq!(unsafe { get_breakindent_win(&mut win, line) }, 50);

        // Changing the line invalidates the cache and forces a
        // genuine recompute.
        let line2 = b"  text\0"; // 2 spaces
        assert_eq!(unsafe { get_breakindent_win(&mut win, line2) }, 2);
    }

    #[test]
    fn get_breakindent_win_briopt_sbr_subtracts_showbreak_width() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { handle: 106, b_p_ts: 8, ..Default::default() };
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        win.w_view_width = 80;
        win.w_briopt_sbr = true;
        win.w_onebuf_opt.wo_sbr = Some(b">>".to_vec());
        let line = b"    text\0"; // indent=4
        // bri = 4 - vim_strsize(">>")(2 printable ASCII cells) = 2.
        assert_eq!(unsafe { get_breakindent_win(&mut win, line) }, 2);
    }

    #[test]
    fn get_breakindent_win_never_indents_past_left_window_margin() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { handle: 107, b_p_ts: 8, ..Default::default() };
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        win.w_view_width = 10;
        win.w_briopt_min = 2;
        let mut line = vec![b' '; 20];
        line.extend_from_slice(b"text\0");
        // indent=20; eff_wwidth=10 (no number/fold/sign/cpo-n columns);
        // clamp to max(10 - 2, 0) = 8.
        assert_eq!(unsafe { get_breakindent_win(&mut win, &line) }, 8);
    }

    #[test]
    #[should_panic(expected = "vim_regcomp")]
    fn get_breakindent_win_briopt_list_panics_needing_regex_engine() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { handle: 108, ..Default::default() };
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        win.w_briopt_list = 1;
        let line = b"1. text\0";
        let _ = unsafe { get_breakindent_win(&mut win, line) };
    }

    #[test]
    fn tabstop_set_empty_string_is_none() {
        assert_eq!(tabstop_set(b""), Ok(None));
    }

    #[test]
    fn tabstop_set_literal_zero_is_none() {
        assert_eq!(tabstop_set(b"0"), Ok(None));
    }

    #[test]
    fn tabstop_set_single_value() {
        assert_eq!(tabstop_set(b"4"), Ok(Some(vec![4])));
    }

    #[test]
    fn tabstop_set_multiple_values() {
        assert_eq!(tabstop_set(b"4,8"), Ok(Some(vec![4, 8])));
        assert_eq!(tabstop_set(b"4,8,12"), Ok(Some(vec![4, 8, 12])));
    }

    #[test]
    fn tabstop_set_rejects_a_negative_value() {
        assert_eq!(tabstop_set(b"-4"), Err(()));
    }

    #[test]
    fn tabstop_set_rejects_a_trailing_comma_with_nothing_after() {
        assert_eq!(tabstop_set(b"4,"), Err(()));
    }

    #[test]
    fn tabstop_set_rejects_a_leading_comma() {
        assert_eq!(tabstop_set(b",4"), Err(()));
    }

    #[test]
    fn tabstop_set_rejects_a_doubled_comma() {
        assert_eq!(tabstop_set(b"4,,8"), Err(()));
    }

    #[test]
    fn tabstop_set_rejects_a_zero_within_a_list() {
        // The "0 means unset" special case only applies to the WHOLE
        // string being exactly "0", not to an individual list entry.
        assert_eq!(tabstop_set(b"4,0"), Err(()));
    }

    #[test]
    fn tabstop_set_rejects_a_non_digit_character() {
        assert_eq!(tabstop_set(b"4,a,8"), Err(()));
    }

    #[test]
    fn tabstop_set_accepts_the_maximum_value() {
        assert_eq!(tabstop_set(b"9999"), Ok(Some(vec![9999])));
    }

    #[test]
    fn tabstop_set_rejects_a_value_past_the_maximum() {
        assert_eq!(tabstop_set(b"10000"), Err(()));
    }

    #[test]
    fn tabstop_padding_plain_tabstop_no_vts() {
        // Matches the original's `ts - (col % ts)`.
        assert_eq!(tabstop_padding(0, 8, None), 8);
        assert_eq!(tabstop_padding(2, 8, None), 6);
        assert_eq!(tabstop_padding(10, 8, None), 6);
    }

    #[test]
    fn tabstop_padding_zero_ts_defaults_to_eight() {
        assert_eq!(tabstop_padding(0, 0, None), 8);
    }

    #[test]
    fn tabstop_padding_vts_within_explicit_stops() {
        // vts = [4, 8] means tab stops at columns 4 and 4+8=12.
        // Hand-traced against the original's own 1-indexed loop
        // (tabcol accumulates vts[1], vts[2], ...; the original's
        // vts[0] merely held the now-implicit .len()).
        assert_eq!(tabstop_padding(0, 8, Some(&[4, 8])), 4); // next stop at 4
        assert_eq!(tabstop_padding(5, 8, Some(&[4, 8])), 7); // next stop at 12
    }

    #[test]
    fn tabstop_padding_vts_beyond_explicit_stops_repeats_last_width() {
        // Beyond the last explicit stop (12), tab stops repeat every
        // 8 columns (the last width) - hand-traced: col=15 is 3 past
        // the stop at 12, so padding = 8 - 3 = 5.
        assert_eq!(tabstop_padding(15, 8, Some(&[4, 8])), 5);
    }

    #[test]
    fn tabstop_padding_vts_empty_slice_falls_back_to_ts() {
        assert_eq!(tabstop_padding(10, 8, Some(&[])), 6);
    }

    #[test]
    fn indent_size_no_ts_counts_spaces_and_treats_tab_as_control_char() {
        assert_eq!(unsafe { indent_size_no_ts(b"  x\0") }, 2);
        // Each TAB is byte2cells(TAB) == 2 cells (control char, no uhex).
        assert_eq!(unsafe { indent_size_no_ts(b"\t\tx\0") }, 4);
        assert_eq!(unsafe { indent_size_no_ts(b"  \tx\0") }, 4);
    }

    #[test]
    fn indent_size_no_ts_stops_immediately_on_non_blank() {
        assert_eq!(unsafe { indent_size_no_ts(b"\0") }, 0);
        assert_eq!(unsafe { indent_size_no_ts(b"x\0") }, 0);
    }

    #[test]
    fn indent_size_ts_fixed_width_counts_spaces() {
        assert_eq!(indent_size_ts(b"  x\0", 8, None), 2);
    }

    #[test]
    fn indent_size_ts_fixed_width_tab_jumps_to_next_stop() {
        assert_eq!(indent_size_ts(b"\tx\0", 8, None), 8);
        // A leading space doesn't change where the following tab lands
        // (still the same 8-column boundary).
        assert_eq!(indent_size_ts(b" \tx\0", 8, None), 8);
    }

    #[test]
    fn indent_size_ts_vts_two_tabs_reach_cumulative_width() {
        // vts=[4, 8]: first tab lands at column 4, second at 4+8=12.
        assert_eq!(indent_size_ts(b"\t\tx\0", 8, Some(&[4, 8])), 12);
    }

    #[test]
    fn indent_size_ts_vts_spaces_stop_before_reaching_a_boundary() {
        // 2 spaces never reach the first vts boundary (4) - the
        // non-blank 'x' stops counting right there at 2.
        assert_eq!(indent_size_ts(b"  x\0", 8, Some(&[4, 8])), 2);
    }

    #[test]
    fn indent_size_ts_vts_spaces_landing_exactly_on_a_boundary() {
        // 4 spaces exactly reach the first vts boundary (4); the
        // following 'x' stops counting right there, not entering the
        // second vts entry at all.
        assert_eq!(indent_size_ts(b"    x\0", 8, Some(&[4, 8])), 4);
    }

    #[test]
    fn indent_size_ts_vts_empty_slice_falls_back_to_fixed_ts() {
        assert_eq!(indent_size_ts(b"\tx\0", 8, Some(&[])), 8);
    }

    /// RAII guard installing `win`/`buf` as curwin/curbuf, restoring
    /// the previous pointers on drop. Holds `global_state_test_lock`
    /// for its entire lifetime, matching `cursor.rs`'s own
    /// `CursorTestGuard` precedent (needed since `ml_open`, used to
    /// build the test memline below, touches shared `GLOBALS.got_int`
    /// internally).
    struct CursorTestGuard {
        prev_curwin: *mut WinT,
        prev_curbuf: *mut BufT,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl CursorTestGuard {
        fn set(win: *mut WinT, buf: *mut BufT) -> Self {
            let _lock = crate::globals::global_state_test_lock();
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            let guard = CursorTestGuard { prev_curwin: globals.curwin, prev_curbuf: globals.curbuf, _lock };
            globals.curwin = win;
            globals.curbuf = buf;
            guard
        }
    }

    impl Drop for CursorTestGuard {
        fn drop(&mut self) {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            globals.curwin = self.prev_curwin;
            globals.curbuf = self.prev_curbuf;
        }
    }

    /// Installs `win`/`buf` as curwin/curbuf, then opens a fresh
    /// memline for `buf` and replaces line 1 with `line` (matching
    /// `cursor.rs`'s own `open_and_set_test_buf` precedent). Callers
    /// must close `buf.b_ml.ml_mfp` themselves after the guard drops.
    fn open_and_set_test_buf(win: &mut WinT, buf: &mut BufT, line: &[u8]) -> CursorTestGuard {
        let guard = CursorTestGuard::set(win as *mut WinT, buf as *mut BufT);
        assert_eq!(unsafe { crate::memline::ml_open(buf) }, crate::vim_defs::OK);
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(buf, 1, line) },
            crate::vim_defs::OK
        );
        guard
    }

    fn close_buf_with_memline(buf: BufT) {
        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn get_indent_counts_the_cursor_lines_indent() {
        let mut buf = BufT { b_p_ts: 8, ..Default::default() };
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"    text\0");
        win.w_cursor.lnum = 1;

        assert_eq!(unsafe { get_indent() }, 4);

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn get_indent_lnum_counts_a_specific_lines_indent() {
        let mut buf = BufT { b_p_ts: 8, ..Default::default() };
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"\ttext\0");

        assert_eq!(unsafe { get_indent_lnum(1) }, 8);

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn get_indent_buf_counts_a_specific_buffers_line_indent() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { b_p_ts: 4, ..Default::default() };
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(&mut buf, 1, b"  text\0") },
            crate::vim_defs::OK
        );

        assert_eq!(unsafe { get_indent_buf(&mut buf, 1) }, 2);

        close_buf_with_memline(buf);
    }

    #[test]
    fn get_indent_family_uses_variable_tabstops_when_set() {
        let mut buf =
            BufT { b_p_ts: 8, b_p_vts_array: Some(vec![4, 8]), ..Default::default() };
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"\t\ttext\0");
        win.w_cursor.lnum = 1;

        // vts=[4, 8]: first tab lands at column 4, second at 4+8=12.
        assert_eq!(unsafe { get_indent() }, 12);
        assert_eq!(unsafe { get_indent_lnum(1) }, 12);

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn f_indent_reads_a_valid_lines_indent() {
        let mut buf = BufT { b_p_ts: 4, ..Default::default() };
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"    text\0");

        let argvars = [crate::eval::typval_defs::TypvalT {
            value: crate::eval::typval_defs::TypvalValue::Number(1),
            ..Default::default()
        }];
        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        unsafe { f_indent(&argvars, &mut rettv) };
        assert_eq!(rettv.value, crate::eval::typval_defs::TypvalValue::Number(4));

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn f_indent_returns_minus_one_for_an_out_of_range_line() {
        let mut buf = BufT { b_p_ts: 4, ..Default::default() };
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"text\0");

        let argvars = [crate::eval::typval_defs::TypvalT {
            value: crate::eval::typval_defs::TypvalValue::Number(99),
            ..Default::default()
        }];
        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        unsafe { f_indent(&argvars, &mut rettv) };
        assert_eq!(rettv.value, crate::eval::typval_defs::TypvalValue::Number(-1));

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn f_indent_returns_minus_one_for_line_zero() {
        let mut buf = BufT { b_p_ts: 4, ..Default::default() };
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"text\0");

        let argvars = [crate::eval::typval_defs::TypvalT {
            value: crate::eval::typval_defs::TypvalValue::Number(0),
            ..Default::default()
        }];
        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        unsafe { f_indent(&argvars, &mut rettv) };
        assert_eq!(rettv.value, crate::eval::typval_defs::TypvalValue::Number(-1));

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn tabstop_at_no_vts_returns_ts() {
        assert_eq!(tabstop_at(0, 8, None, false), 8);
        assert_eq!(tabstop_at(5, 4, None, false), 4);
    }

    #[test]
    fn tabstop_at_within_explicit_stops() {
        // vts = [4, 8]: tab stops at columns 4 and 4+8=12.
        assert_eq!(tabstop_at(2, 8, Some(&[4, 8]), false), 4);
        assert_eq!(tabstop_at(5, 8, Some(&[4, 8]), false), 8);
    }

    #[test]
    fn tabstop_at_beyond_explicit_stops_repeats_last_width() {
        assert_eq!(tabstop_at(15, 8, Some(&[4, 8]), false), 8);
    }

    #[test]
    fn tabstop_at_left_true_at_column_zero_returns_col() {
        // Shifting left from column 0 (before the first tabstop):
        // the original returns `col` itself in this special case.
        assert_eq!(tabstop_at(0, 8, Some(&[4, 8]), true), 0);
    }

    #[test]
    fn tabstop_at_left_true_returns_the_previous_stop_width() {
        // col=6 is between the stop at 4 and the stop at 12; shifting
        // left returns the width of the PRECEDING interval (4).
        assert_eq!(tabstop_at(6, 8, Some(&[4, 8]), true), 4);
    }

    #[test]
    fn tabstop_at_empty_vts_falls_back_to_ts() {
        assert_eq!(tabstop_at(10, 8, Some(&[]), false), 8);
    }

    #[test]
    fn tabstop_start_no_vts_rounds_down_to_a_tab_boundary() {
        assert_eq!(tabstop_start(10, 8, None), 8);
        assert_eq!(tabstop_start(8, 8, None), 8);
        assert_eq!(tabstop_start(5, 8, None), 0);
    }

    #[test]
    fn tabstop_start_within_explicit_stops() {
        // vts = [4, 8]: tab stops at columns 4 and 4+8=12.
        assert_eq!(tabstop_start(2, 8, Some(&[4, 8])), 0);
        assert_eq!(tabstop_start(5, 8, Some(&[4, 8])), 4);
    }

    #[test]
    fn tabstop_start_beyond_explicit_stops_repeats_last_width() {
        // Beyond the last explicit stop (12), tabs repeat every 8
        // columns: 12, 20, 28, ... col=15 falls in [12,20) -> 12;
        // col=20 sits exactly on the next boundary -> starts there.
        assert_eq!(tabstop_start(15, 8, Some(&[4, 8])), 12);
        assert_eq!(tabstop_start(20, 8, Some(&[4, 8])), 20);
    }

    #[test]
    fn tabstop_start_empty_vts_falls_back_to_ts() {
        assert_eq!(tabstop_start(10, 8, Some(&[])), 8);
    }

    #[test]
    fn tabstop_fromto_no_vts_simple_case() {
        // start=0, end=10, ts=8: 1 tab (0->8) + 2 spaces (8->10).
        assert_eq!(unsafe { tabstop_fromto(0, 10, 8, None) }, (1, 2));
    }

    #[test]
    fn tabstop_fromto_no_vts_from_a_non_boundary_start() {
        // start=2, end=10, ts=8: 1 tab (2->8) + 2 spaces (8->10).
        assert_eq!(unsafe { tabstop_fromto(2, 10, 8, None) }, (1, 2));
    }

    #[test]
    fn tabstop_fromto_no_vts_not_enough_room_for_a_tab() {
        // start=2, end=5, ts=8: never reaches the tabstop at 8, so 3
        // plain spaces only.
        assert_eq!(unsafe { tabstop_fromto(2, 5, 8, None) }, (0, 3));
    }

    #[test]
    fn tabstop_fromto_no_vts_multiple_tabs() {
        // start=0, end=25, ts=8: 3 tabs (0->8->16->24) + 1 space.
        assert_eq!(unsafe { tabstop_fromto(0, 25, 8, None) }, (3, 1));
    }

    #[test]
    fn tabstop_fromto_zero_ts_arg_uses_curbufs_own_tabstop() {
        let mut buf = BufT { b_p_ts: 4, ..Default::default() };
        // Only `GLOBALS.curbuf` is touched (not `curwin`), matching
        // this function's own narrower safety doc - `win` is null.
        let _guard = CursorTestGuard::set(std::ptr::null_mut(), &mut buf as *mut BufT);
        // ts=4 (from curbuf.b_p_ts): start=0,end=10 -> 2 tabs (0->4->8)
        // + 2 spaces (8->10).
        assert_eq!(unsafe { tabstop_fromto(0, 10, 0, None) }, (2, 2));
    }

    #[test]
    fn tabstop_fromto_vts_within_explicit_stops() {
        // vts=[4,8]: start=0,end=6 -> 1 tab (0->4) + 2 spaces (4->6);
        // the next stop (12) needs 8 more spaces, out of reach.
        assert_eq!(unsafe { tabstop_fromto(0, 6, 8, Some(&[4, 8])) }, (1, 2));
    }

    #[test]
    fn tabstop_fromto_vts_spans_multiple_explicit_stops() {
        // vts=[4,8]: start=0,end=14 -> 2 tabs (0->4->12) + 2 spaces.
        assert_eq!(unsafe { tabstop_fromto(0, 14, 8, Some(&[4, 8])) }, (2, 2));
    }

    #[test]
    fn tabstop_fromto_vts_starting_past_the_first_explicit_stop() {
        // vts=[4,8]: start=5 (past the stop at 4), end=30 -> 3 tabs
        // (5->12->20->28) + 2 spaces (28->30).
        assert_eq!(unsafe { tabstop_fromto(5, 30, 8, Some(&[4, 8])) }, (3, 2));
    }

    #[test]
    fn tabstop_fromto_vts_not_enough_room_for_even_one_tab() {
        // vts=[4,8]: start=0,end=2 -> can't reach the first stop (4).
        assert_eq!(unsafe { tabstop_fromto(0, 2, 8, Some(&[4, 8])) }, (0, 2));
    }

    #[test]
    fn tabstop_fromto_empty_vts_falls_back_to_ts() {
        assert_eq!(unsafe { tabstop_fromto(0, 10, 8, Some(&[])) }, (1, 2));
    }

    #[test]
    fn get_sw_value_col_uses_shiftwidth_when_nonzero() {
        let buf = BufT { b_p_sw: 4, b_p_ts: 8, ..Default::default() };
        // b_p_sw takes priority; col/left are ignored.
        assert_eq!(get_sw_value_col(&buf, 99, true), 4);
    }

    #[test]
    fn get_sw_value_col_falls_back_to_tabstop_at_when_shiftwidth_is_zero() {
        let buf = BufT { b_p_sw: 0, b_p_ts: 8, b_p_vts_array: Some(vec![4, 8]), ..Default::default() };
        assert_eq!(get_sw_value_col(&buf, 2, false), 4);
    }

    #[test]
    fn get_sw_value_matches_get_sw_value_col_at_column_zero() {
        let buf = BufT { b_p_sw: 0, b_p_ts: 8, b_p_vts_array: Some(vec![4, 8]), ..Default::default() };
        assert_eq!(get_sw_value(&buf), get_sw_value_col(&buf, 0, false));
        assert_eq!(get_sw_value(&buf), 4);
    }

    #[test]
    fn get_sw_value_pos_saves_and_restores_the_cursor_without_a_real_memline() {
        // Exercises `get_sw_value_pos`'s own raw-pointer cursor
        // save/restore dance WITHOUT needing `ml_open` (kept as its
        // own, separate, minimal test specifically so `cargo miri
        // test` can verify this function's pointer manipulation is
        // sound even though `open_and_set_test_buf`'s own `ml_open`
        // call hits this crate's already-documented, pre-existing
        // `libc::getpwuid` Miri-FFI limitation - see the sibling test
        // below). `win.w_buffer` is deliberately left null, so
        // `get_nolist_virtcol`'s own early-return kicks in (`col`
        // always 0) - this test is about the pointer manipulation
        // itself, not a specific virtual-column value.
        let mut buf = BufT { b_p_sw: 4, ..Default::default() };
        let mut win = WinT::default();
        let guard = CursorTestGuard::set(&mut win as *mut WinT, &mut buf as *mut BufT);
        unsafe {
            (*crate::globals::GLOBALS.get_mut().curwin).w_cursor =
                crate::pos_defs::PosT { lnum: 5, col: 3, coladd: 0 };
        }

        let buf_ptr = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        let target_pos = crate::pos_defs::PosT { lnum: 1, col: 0, coladd: 0 };
        // b_p_sw=4 (nonzero) takes priority regardless of `col`.
        assert_eq!(unsafe { get_sw_value_pos(buf_ptr, &target_pos, false) }, 4);

        // The cursor is restored to its ORIGINAL value afterward.
        let restored = unsafe { (*crate::globals::GLOBALS.get_mut().curwin).w_cursor };
        assert_eq!(restored, crate::pos_defs::PosT { lnum: 5, col: 3, coladd: 0 });

        drop(guard);
    }

    #[test]
    fn get_sw_value_pos_uses_the_given_positions_column_and_restores_the_cursor() {
        let mut buf = BufT { b_p_sw: 0, b_p_ts: 8, b_p_vts_array: Some(vec![4, 8]), ..Default::default() };
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"hello\0");
        win.w_cursor = crate::pos_defs::PosT { lnum: 1, col: 2, coladd: 0 };
        // Wire `win.w_buffer` from `GLOBALS.curbuf`'s own stored
        // value (never re-borrowed independently from `buf`), same
        // discipline as `win_text_height`'s established `wref`
        // pattern elsewhere in this crate.
        unsafe {
            let g = crate::globals::GLOBALS.get_mut();
            (*g.curwin).w_buffer = g.curbuf;
        }

        let buf_ptr = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        // No 'list', no 'cpo' L flag -> get_nolist_virtcol resolves
        // via getvcol_nolist, matching plain byte columns here (all
        // ASCII, no tabs) - target col=0, so tabstop_at(0, ts=8,
        // vts=[4,8], left=false) = 4 (the first explicit stop).
        let target_pos = crate::pos_defs::PosT { lnum: 1, col: 0, coladd: 0 };
        let sw = unsafe { get_sw_value_pos(buf_ptr, &target_pos, false) };
        assert_eq!(sw, 4);

        // The cursor is restored to its original column afterward.
        let restored_col = unsafe { (*crate::globals::GLOBALS.get_mut().curwin).w_cursor.col };
        assert_eq!(restored_col, 2);

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn get_sw_value_pos_uses_the_real_shiftwidth_when_nonzero() {
        let mut buf = BufT { b_p_sw: 4, b_p_ts: 8, ..Default::default() };
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"hello\0");
        win.w_cursor = crate::pos_defs::PosT { lnum: 1, col: 0, coladd: 0 };
        unsafe {
            let g = crate::globals::GLOBALS.get_mut();
            (*g.curwin).w_buffer = g.curbuf;
        }

        let buf_ptr = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        let target_pos = crate::pos_defs::PosT { lnum: 1, col: 3, coladd: 0 };
        // b_p_sw=4 (nonzero) always takes priority, regardless of
        // the target column.
        assert_eq!(unsafe { get_sw_value_pos(buf_ptr, &target_pos, false) }, 4);

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn get_sw_value_indent_uses_the_first_non_blank_column() {
        let mut buf = BufT { b_p_sw: 0, b_p_ts: 8, ..Default::default() };
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"    text\0");
        win.w_cursor.lnum = 1;
        unsafe {
            let g = crate::globals::GLOBALS.get_mut();
            (*g.curwin).w_buffer = g.curbuf;
        }

        let buf_ptr = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        // The first non-blank is at column 4; no vts, so
        // tabstop_at(4, ts=8, None, left=false) = 8.
        assert_eq!(unsafe { get_sw_value_indent(buf_ptr, false) }, 8);

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn get_sw_value_indent_left_true_returns_the_preceding_intervals_width() {
        let mut buf =
            BufT { b_p_sw: 0, b_p_ts: 8, b_p_vts_array: Some(vec![4, 8]), ..Default::default() };
        let mut win = WinT::default();
        // First non-blank at column 6 (between the stop at 4 and the
        // stop at 12) - shifting left returns the PRECEDING
        // interval's width (4), matching `tabstop_at`'s own
        // `left=true` semantics.
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"      text\0");
        win.w_cursor.lnum = 1;
        unsafe {
            let g = crate::globals::GLOBALS.get_mut();
            (*g.curwin).w_buffer = g.curbuf;
        }

        let buf_ptr = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        assert_eq!(unsafe { get_sw_value_indent(buf_ptr, true) }, 4);

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn get_sts_value_uses_softtabstop_when_non_negative() {
        let mut buf = BufT { b_p_sts: 4, b_p_sw: 8, ..Default::default() };
        let _guard = CursorTestGuard::set(std::ptr::null_mut(), &mut buf as *mut BufT);
        assert_eq!(unsafe { get_sts_value() }, 4);
    }

    #[test]
    fn get_sts_value_zero_is_returned_as_is() {
        let mut buf = BufT { b_p_sts: 0, b_p_sw: 8, ..Default::default() };
        let _guard = CursorTestGuard::set(std::ptr::null_mut(), &mut buf as *mut BufT);
        assert_eq!(unsafe { get_sts_value() }, 0);
    }

    #[test]
    fn get_sts_value_negative_falls_back_to_shiftwidth() {
        let mut buf = BufT { b_p_sts: -1, b_p_sw: 8, ..Default::default() };
        let _guard = CursorTestGuard::set(std::ptr::null_mut(), &mut buf as *mut BufT);
        assert_eq!(unsafe { get_sts_value() }, 8);
    }

    #[test]
    fn get_sts_value_negative_falls_back_to_tabstop_at_when_shiftwidth_is_zero() {
        let mut buf = BufT { b_p_sts: -1, b_p_sw: 0, b_p_ts: 4, ..Default::default() };
        let _guard = CursorTestGuard::set(std::ptr::null_mut(), &mut buf as *mut BufT);
        assert_eq!(unsafe { get_sts_value() }, 4);
    }

    #[test]
    fn inindent_cursor_before_first_non_blank_is_true() {
        let mut buf = BufT::default();
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"    text\0");
        unsafe { (*crate::globals::GLOBALS.get_mut().curwin).w_cursor.col = 2 };

        assert!(unsafe { inindent(0) });

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn inindent_cursor_exactly_on_first_non_blank_extra_zero_is_true() {
        let mut buf = BufT::default();
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"    text\0");
        unsafe { (*crate::globals::GLOBALS.get_mut().curwin).w_cursor.col = 4 };

        assert!(unsafe { inindent(0) });

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn inindent_cursor_exactly_on_first_non_blank_extra_one_is_false() {
        // With extra=1, the cursor must be STRICTLY before the first
        // non-blank, not on it.
        let mut buf = BufT::default();
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"    text\0");
        unsafe { (*crate::globals::GLOBALS.get_mut().curwin).w_cursor.col = 4 };

        assert!(!unsafe { inindent(1) });

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn inindent_cursor_past_first_non_blank_is_false() {
        let mut buf = BufT::default();
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"    text\0");
        unsafe { (*crate::globals::GLOBALS.get_mut().curwin).w_cursor.col = 5 };

        assert!(!unsafe { inindent(0) });

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn inindent_empty_line_cursor_at_zero_is_true() {
        let mut buf = BufT::default();
        let mut win = WinT::default();
        let guard = open_and_set_test_buf(&mut win, &mut buf, b"\0");

        assert!(unsafe { inindent(0) });

        drop(guard);
        close_buf_with_memline(buf);
    }

    #[test]
    fn lisp_match_finds_a_word_from_the_global_lispwords() {
        let mut buf = BufT { b_p_lw: None, ..Default::default() };
        let _guard = CursorTestGuard::set(std::ptr::null_mut(), &mut buf as *mut BufT);
        let prev = unsafe { crate::option_vars::OPTION_VARS.get_mut() }
            .p_lispwords
            .clone();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_lispwords =
            Some(b"defun,let,if".to_vec());

        assert!(unsafe { lisp_match(b"defun foo") });
        assert!(unsafe { lisp_match(b"let x") });
        assert!(!unsafe { lisp_match(b"lambda x") });

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_lispwords = prev;
    }

    #[test]
    fn lisp_match_requires_a_whitespace_or_nul_boundary() {
        // A longer identifier sharing a prefix must NOT match.
        let mut buf = BufT { b_p_lw: None, ..Default::default() };
        let _guard = CursorTestGuard::set(std::ptr::null_mut(), &mut buf as *mut BufT);
        let prev = unsafe { crate::option_vars::OPTION_VARS.get_mut() }
            .p_lispwords
            .clone();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_lispwords =
            Some(b"let".to_vec());

        assert!(!unsafe { lisp_match(b"letter x") });
        assert!(unsafe { lisp_match(b"let\0") }, "NUL counts as a boundary");

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_lispwords = prev;
    }

    #[test]
    fn lisp_match_prefers_the_buffer_local_lispwords() {
        let mut buf = BufT {
            b_p_lw: Some(b"local".to_vec()),
            ..Default::default()
        };
        let _guard = CursorTestGuard::set(std::ptr::null_mut(), &mut buf as *mut BufT);
        let prev = unsafe { crate::option_vars::OPTION_VARS.get_mut() }
            .p_lispwords
            .clone();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_lispwords =
            Some(b"global".to_vec());

        assert!(unsafe { lisp_match(b"local x") });
        assert!(
            !unsafe { lisp_match(b"global x") },
            "the local option shadows the global one entirely"
        );

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_lispwords = prev;
    }

    #[test]
    fn lisp_match_false_when_no_lispwords_are_set() {
        let mut buf = BufT { b_p_lw: None, ..Default::default() };
        let _guard = CursorTestGuard::set(std::ptr::null_mut(), &mut buf as *mut BufT);
        let prev = unsafe { crate::option_vars::OPTION_VARS.get_mut() }
            .p_lispwords
            .clone();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_lispwords = None;

        assert!(!unsafe { lisp_match(b"defun foo") });

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_lispwords = prev;
    }

    #[test]
    fn preprocs_left_true_with_smartindent_and_no_cindent() {
        let mut buf = BufT { b_p_si: 1, b_p_cin: 0, ..Default::default() };
        let _guard = CursorTestGuard::set(std::ptr::null_mut(), &mut buf as *mut BufT);
        assert!(unsafe { preprocs_left() });
    }

    #[test]
    fn preprocs_left_false_with_neither_option() {
        let mut buf = BufT { b_p_si: 0, b_p_cin: 0, ..Default::default() };
        let _guard = CursorTestGuard::set(std::ptr::null_mut(), &mut buf as *mut BufT);
        assert!(!unsafe { preprocs_left() });
    }

    #[test]
    #[should_panic(expected = "in_cinkeys")]
    fn preprocs_left_cindent_branch_is_unreachable_but_documented() {
        // Proves the guard is really driven by 'cindent' rather than
        // hardcoded away: forcing a value nothing in this crate can
        // currently produce reaches the deferred branch.
        let mut buf = BufT { b_p_si: 0, b_p_cin: 1, ..Default::default() };
        let _guard = CursorTestGuard::set(std::ptr::null_mut(), &mut buf as *mut BufT);
        let _ = unsafe { preprocs_left() };
    }

    #[test]
    fn may_do_si_true_when_every_condition_holds() {
        let mut buf =
            BufT { b_p_si: 1, b_p_cin: 0, b_p_inde: None, ..Default::default() };
        let _guard = CursorTestGuard::set(std::ptr::null_mut(), &mut buf as *mut BufT);
        assert!(unsafe { may_do_si() });
    }

    #[test]
    fn may_do_si_false_when_smartindent_is_off() {
        let mut buf = BufT { b_p_si: 0, ..Default::default() };
        let _guard = CursorTestGuard::set(std::ptr::null_mut(), &mut buf as *mut BufT);
        assert!(!unsafe { may_do_si() });
    }

    #[test]
    fn may_do_si_false_when_cindent_is_on() {
        let mut buf = BufT { b_p_si: 1, b_p_cin: 1, ..Default::default() };
        let _guard = CursorTestGuard::set(std::ptr::null_mut(), &mut buf as *mut BufT);
        assert!(!unsafe { may_do_si() });
    }

    #[test]
    fn may_do_si_false_when_indentexpr_is_set() {
        let mut buf = BufT {
            b_p_si: 1,
            b_p_cin: 0,
            b_p_inde: Some(b"MyIndent()".to_vec()),
            ..Default::default()
        };
        let _guard = CursorTestGuard::set(std::ptr::null_mut(), &mut buf as *mut BufT);
        assert!(!unsafe { may_do_si() });
    }

    #[test]
    fn may_do_si_false_when_paste_is_on() {
        let mut buf = BufT { b_p_si: 1, b_p_cin: 0, b_p_inde: None, ..Default::default() };
        let _guard = CursorTestGuard::set(std::ptr::null_mut(), &mut buf as *mut BufT);
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_paste = 1;
        let result = unsafe { may_do_si() };
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_paste = 0;
        assert!(!result);
    }

    // --- briopt_check ---

    #[test]
    fn briopt_check_empty_string_succeeds_with_defaults() {
        let mut win = WinT::default();
        assert!(briopt_check(Some(b""), Some(&mut win)));
        assert_eq!(win.w_briopt_shift, 0);
        assert_eq!(win.w_briopt_min, 20);
        assert!(!win.w_briopt_sbr);
        assert_eq!(win.w_briopt_list, 0);
        assert_eq!(win.w_briopt_vcol, 0);
    }

    #[test]
    fn briopt_check_shift_positive() {
        let mut win = WinT::default();
        assert!(briopt_check(Some(b"shift:5"), Some(&mut win)));
        assert_eq!(win.w_briopt_shift, 5);
    }

    #[test]
    fn briopt_check_shift_negative() {
        let mut win = WinT::default();
        assert!(briopt_check(Some(b"shift:-3"), Some(&mut win)));
        assert_eq!(win.w_briopt_shift, -3);
    }

    #[test]
    fn briopt_check_min() {
        let mut win = WinT::default();
        assert!(briopt_check(Some(b"min:10"), Some(&mut win)));
        assert_eq!(win.w_briopt_min, 10);
    }

    #[test]
    fn briopt_check_sbr() {
        let mut win = WinT::default();
        assert!(briopt_check(Some(b"sbr"), Some(&mut win)));
        assert!(win.w_briopt_sbr);
    }

    #[test]
    fn briopt_check_list() {
        let mut win = WinT::default();
        assert!(briopt_check(Some(b"list:2"), Some(&mut win)));
        assert_eq!(win.w_briopt_list, 2);
    }

    #[test]
    fn briopt_check_column() {
        let mut win = WinT::default();
        assert!(briopt_check(Some(b"column:15"), Some(&mut win)));
        assert_eq!(win.w_briopt_vcol, 15);
    }

    #[test]
    fn briopt_check_multiple_comma_separated_entries() {
        let mut win = WinT::default();
        assert!(briopt_check(Some(b"shift:5,min:10,sbr"), Some(&mut win)));
        assert_eq!(win.w_briopt_shift, 5);
        assert_eq!(win.w_briopt_min, 10);
        assert!(win.w_briopt_sbr);
        // Unset entries keep their own defaults.
        assert_eq!(win.w_briopt_list, 0);
        assert_eq!(win.w_briopt_vcol, 0);
    }

    #[test]
    fn briopt_check_unrecognized_entry_fails_and_does_not_touch_wp() {
        let mut win = WinT { w_briopt_shift: 99, ..Default::default() };
        assert!(!briopt_check(Some(b"bogus"), Some(&mut win)));
        // A failed parse never writes anything back into wp.
        assert_eq!(win.w_briopt_shift, 99);
    }

    #[test]
    fn briopt_check_shift_prefix_without_a_valid_digit_is_unrecognized() {
        // "shift:x" textually starts with the "shift:" keyword, but
        // the original's own guard requires a real digit (or "-"
        // followed by a digit) right after it - "x" doesn't qualify,
        // so the whole entry is treated as wholly unrecognized (not
        // partially consumed).
        let mut win = WinT::default();
        assert!(!briopt_check(Some(b"shift:x"), Some(&mut win)));
    }

    #[test]
    fn briopt_check_wp_none_only_validates_without_needing_a_window() {
        assert!(briopt_check(Some(b"shift:5,sbr"), None));
        assert!(!briopt_check(Some(b"bogus"), None));
    }

    #[test]
    fn briopt_check_falls_back_to_wp_own_option_value_when_briopt_is_none() {
        let mut win = WinT { w_onebuf_opt: crate::buffer_defs::WinoptT { wo_briopt: Some(b"shift:7".to_vec()), ..Default::default() }, ..Default::default() };
        assert!(briopt_check(None, Some(&mut win)));
        assert_eq!(win.w_briopt_shift, 7);
    }

    #[test]
    fn briopt_check_no_briopt_and_no_wp_uses_the_empty_string() {
        assert!(briopt_check(None, None));
    }

    #[test]
    fn shiftwidth_with_no_args_uses_get_sw_value() {
        let mut buf = BufT { b_p_sw: 4, ..Default::default() };
        let _guard = CursorTestGuard::set(std::ptr::null_mut(), &mut buf as *mut BufT);

        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        unsafe { f_shiftwidth(&[], &mut rettv) };
        assert_eq!(rettv.value, crate::eval::typval_defs::TypvalValue::Number(4));
    }

    #[test]
    fn shiftwidth_with_col_uses_get_sw_value_col() {
        let mut buf = BufT { b_p_sw: 0, b_p_ts: 8, b_p_vts_array: Some(vec![4, 8]), ..Default::default() };
        let _guard = CursorTestGuard::set(std::ptr::null_mut(), &mut buf as *mut BufT);

        let argvars = [crate::eval::typval_defs::TypvalT {
            value: crate::eval::typval_defs::TypvalValue::Number(2),
            ..Default::default()
        }];
        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        unsafe { f_shiftwidth(&argvars, &mut rettv) };
        assert_eq!(rettv.value, crate::eval::typval_defs::TypvalValue::Number(4));
    }

    #[test]
    fn shiftwidth_negative_col_leaves_rettv_at_zero() {
        let mut buf = BufT { b_p_sw: 4, ..Default::default() };
        let _guard = CursorTestGuard::set(std::ptr::null_mut(), &mut buf as *mut BufT);

        let argvars = [crate::eval::typval_defs::TypvalT {
            value: crate::eval::typval_defs::TypvalValue::Number(-1),
            ..Default::default()
        }];
        let mut rettv = crate::eval::typval_defs::TypvalT::default();
        unsafe { f_shiftwidth(&argvars, &mut rettv) };
        assert_eq!(rettv.value, crate::eval::typval_defs::TypvalValue::Number(0));
    }
}
