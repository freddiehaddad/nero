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
}
