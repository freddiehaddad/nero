//! Translated from `src/nvim/drawscreen.c` (tractable core only).
//!
//! `drawscreen.c` is neovim's actual screen-redraw driver (thousands
//! of lines) - almost entirely dependent on the real TUI/grid
//! rendering pipeline, not attempted here. Only the one genuinely
//! self-contained function needed by `plines.c`'s/`move.c`'s window-
//! column-offset calculations is translated: `number_width`.
//!
//! Deferred: everything else in the file.

use crate::buffer_defs::WinT;
use crate::pos_defs::LinenrT;

/// Return the required width for the `'number'`/`'relativenumber'`
/// column in `wp`, caching the result until the relevant line count
/// changes (`number_width`).
///
/// # Safety
/// `wp.w_buffer` must be a valid, non-null pointer to a live `BufT`.
pub unsafe fn number_width(wp: &mut WinT) -> i32 {
    let mut lnum: LinenrT = if wp.w_onebuf_opt.wo_rnu != 0 && wp.w_onebuf_opt.wo_nu == 0 {
        // cursor line shows "0"
        wp.w_view_height
    } else {
        // cursor line shows absolute line number
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &*wp.w_buffer }.b_ml.ml_line_count
    };

    if lnum == wp.w_nrwidth_line_count {
        return wp.w_nrwidth_width;
    }
    wp.w_nrwidth_line_count = lnum;

    // reset for 'statuscolumn'
    if wp.w_onebuf_opt.wo_stc.as_deref().is_some_and(|s| !s.is_empty()) {
        wp.w_statuscol_line_count = 0; // make sure width is re-estimated
        let nu_or_rnu = wp.w_onebuf_opt.wo_nu != 0 || wp.w_onebuf_opt.wo_rnu != 0;
        wp.w_nrwidth_width = i32::from(nu_or_rnu) * (wp.w_onebuf_opt.wo_nuw as i32);
        return wp.w_nrwidth_width;
    }

    let mut n = 0;
    loop {
        lnum /= 10;
        n += 1;
        if lnum <= 0 {
            break;
        }
    }

    // 'numberwidth' gives the minimal width plus one
    n = n.max(wp.w_onebuf_opt.wo_nuw as i32 - 1);

    // If 'signcolumn' is set to 'number' and there is a sign to display, then
    // the minimal width for the number column is 2.
    // SAFETY: forwarded from this function's own safety doc.
    let buf = unsafe { &*wp.w_buffer };
    if n < 2
        && crate::buffer::buf_meta_total(buf, crate::marktree_defs::MetaIndex::SignText) != 0
        && wp.w_minscwidth == crate::option_vars::SCL_NUM
    {
        n = 2;
    }

    wp.w_nrwidth_width = n;
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer_defs::BufT;

    #[test]
    fn number_width_absolute_mode_counts_digits() {
        let mut buf = BufT { ..Default::default() };
        buf.b_ml.ml_line_count = 123;
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        // 123 has 3 digits; 'numberwidth' defaults to 0 here so the
        // `n.max(nuw - 1)` clamp doesn't raise it.
        assert_eq!(unsafe { number_width(&mut win) }, 3);
    }

    #[test]
    fn number_width_caches_until_line_count_changes() {
        let mut buf = BufT { ..Default::default() };
        buf.b_ml.ml_line_count = 5;
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        assert_eq!(unsafe { number_width(&mut win) }, 1);

        // Corrupt the cached width directly - if a second call with the
        // SAME line count truly hits the cache (rather than recomputing),
        // it returns this corrupted value instead of the real one (1).
        win.w_nrwidth_width = 99;
        assert_eq!(unsafe { number_width(&mut win) }, 99);

        // Changing the line count invalidates the cache and forces a
        // genuine recompute.
        buf.b_ml.ml_line_count = 50000;
        assert_eq!(unsafe { number_width(&mut win) }, 5);
    }

    #[test]
    fn number_width_relativenumber_without_number_uses_view_height() {
        let mut buf = BufT { ..Default::default() };
        buf.b_ml.ml_line_count = 999_999; // irrelevant in this mode
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        win.w_onebuf_opt.wo_rnu = 1;
        win.w_onebuf_opt.wo_nu = 0;
        win.w_view_height = 42; // 2 digits

        assert_eq!(unsafe { number_width(&mut win) }, 2);
    }

    #[test]
    fn number_width_numberwidth_option_raises_minimum() {
        let mut buf = BufT { ..Default::default() };
        buf.b_ml.ml_line_count = 5; // 1 digit
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        win.w_onebuf_opt.wo_nuw = 6; // minimal width is nuw - 1 = 5

        assert_eq!(unsafe { number_width(&mut win) }, 5);
    }

    #[test]
    fn number_width_statuscolumn_set_uses_nu_rnu_times_nuw() {
        let mut buf = BufT { ..Default::default() };
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        win.w_onebuf_opt.wo_stc = Some(b"%n".to_vec());
        win.w_onebuf_opt.wo_nu = 1;
        win.w_onebuf_opt.wo_nuw = 4;
        // Force a cache-miss on this first call - lnum computed from the
        // default ml_line_count (0) would otherwise trivially equal the
        // also-defaulted w_nrwidth_line_count (0), short-circuiting before
        // the statuscolumn branch below is ever reached.
        win.w_nrwidth_line_count = -1;

        assert_eq!(unsafe { number_width(&mut win) }, 4);
        assert_eq!(win.w_statuscol_line_count, 0);
    }

    #[test]
    fn number_width_statuscolumn_set_without_nu_or_rnu_is_zero() {
        let mut buf = BufT { ..Default::default() };
        let mut win = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        win.w_onebuf_opt.wo_stc = Some(b"%n".to_vec());
        win.w_onebuf_opt.wo_nuw = 4;
        // Same cache-miss forcing as above.
        win.w_nrwidth_line_count = -1;

        assert_eq!(unsafe { number_width(&mut win) }, 0);
    }
}
