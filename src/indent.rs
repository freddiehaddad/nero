//! Translated from `src/nvim/indent.c` (tractable core only).
//!
//! `indent.c` (~2000 lines) is the auto-indent/`'shiftwidth'`/tab-stop
//! computation file. Most of it needs real buffer-modification
//! (`ml_replace`/`changed_bytes`) plus the C-indent (`indent_c.c`) and
//! Lisp-indent engines.
//!
//! Translated: `tabstop_padding`, `indent_size_no_ts`/`indent_size_ts`
//! (needed by `plines.c`'s tab-width calculations and by
//! `get_breakindent_win` below); `get_breakindent_win` (needed
//! `buffer.c`'s `buf_get_changedtick`, now tractable since
//! `eval/typval_defs.rs`'s `TypvalT` is real - see that function's own
//! doc comment for its one deliberate gap, `'breakindentopt'="list"`,
//! which needs the real regex engine).
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
//! choice for this call site.
//!
//! Deferred: everything else in the file.

use crate::buffer_defs::WinT;
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
}
