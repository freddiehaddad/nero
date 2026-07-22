//! Translated from `src/nvim/indent.c` (tractable core only).
//!
//! `indent.c` (~2000 lines) is the auto-indent/`'shiftwidth'`/tab-stop
//! computation file. Most of it needs real buffer-modification
//! (`ml_replace`/`changed_bytes`) plus the C-indent (`indent_c.c`) and
//! Lisp-indent engines. Only the one genuinely self-contained function
//! needed by `plines.c`'s tab-width calculations is translated here:
//! `tabstop_padding`.
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

use crate::pos_defs::ColnrT;
use crate::types_defs::OptInt;

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

#[cfg(test)]
mod tests {
    use super::*;

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
