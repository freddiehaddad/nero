//! Translated from `src/nvim/ex_session.c` (tractable core only).
//!
//! `ex_session.c` implements `:mksession` - writing out a real session
//! file that restores the current editor layout (real `FILE`-based
//! I/O, real `fprintf`-style formatting of Ex commands and file
//! names) - not tractable here: needs the general printf/formatting
//! engine (`vim_snprintf`, confirmed a MAJOR undertaking elsewhere),
//! real file writing, and `optionstr.c`'s whole `did_set_*` callback
//! machinery for restoring options on `:source`.
//!
//! Translated: [`ses_do_win`]/[`ses_do_frame`]/[`ses_skipframe`] - the
//! pure predicates deciding WHETHER a given window/frame should be
//! included in the session at all (based on `'sessionoptions'` and
//! the window's own buffer type), genuinely self-contained and
//! needing none of the FILE-writing machinery above. Needed only
//! already-real `buffer::bt_help`/`bt_terminal`/`bt_nofilename`,
//! `option_vars::OPTION_VARS.ssop_flags`/`opt_ssop_flag::*`, and
//! `FrameT`/`WinT`'s own already-real fields. No real translated
//! caller yet (every real caller lives in this same file's own
//! FILE-writing functions, none translated) - harvested ahead of it
//! anyway, matching this crate's established "small, simple, no
//! design freedom" ahead-of-caller precedent (e.g. `mark.rs`'s
//! `tagstack_clear_entry`, `undo.rs`'s `u_save_line_buf`).
//!
//! Deferred: everything else - `put_view_curpos`/`ses_winsizes`/
//! `ses_win_rec`/`ses_arglist`/`ses_get_fname`/`ses_fname`/
//! `ses_escape_fname`/`ses_put_fname`/`put_view`/
//! `store_session_globals`/`makeopens`/`get_view_file` (all need real
//! `FILE` writing + `vim_snprintf`-style formatting).

use crate::buffer_defs::{FrameT, WinT, FR_LEAF};
use crate::option_vars::opt_ssop_flag;

/// Whether window `wp` should be stored in the session
/// (`ses_do_win`).
///
/// # Safety
/// `wp` must be a valid, non-null pointer to a live `WinT` whose own
/// `w_buffer` is also a valid, non-null pointer to a live `BufT`.
#[must_use]
pub unsafe fn ses_do_win(wp: *const WinT) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let wp_ref = unsafe { &*wp };
    // Skip floating windows to avoid issues when restoring the
    // session (matches the original's own comment/behavior exactly).
    if wp_ref.w_floating {
        return false;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let buf = unsafe { &*wp_ref.w_buffer };
    // SAFETY: a plain `u32` copy-out read, no aliasing hazard.
    let ssop_flags = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.ssop_flags;

    if buf.b_fname.is_none()
        // When 'buftype' is "nofile" can't restore the window contents.
        || (buf.terminal.is_null() && crate::buffer::bt_nofilename(Some(buf)))
    {
        return ssop_flags & opt_ssop_flag::BLANK != 0;
    }
    if crate::buffer::bt_help(Some(buf)) {
        return ssop_flags & opt_ssop_flag::HELP != 0;
    }
    if crate::buffer::bt_terminal(Some(buf)) {
        return ssop_flags & opt_ssop_flag::TERMINAL != 0;
    }
    true
}

/// Whether frame `fr` has a window somewhere that should be stored in
/// the session (`ses_do_frame`).
///
/// # Safety
/// `fr` must be a valid, non-null pointer to a live `FrameT`, and so
/// must every frame reachable via its own `fr_child`/`fr_next` chain;
/// a leaf frame's own `fr_win` must be a valid, non-null pointer to a
/// live `WinT` (matching every leaf frame in a real window layout).
#[must_use]
pub unsafe fn ses_do_frame(fr: *const FrameT) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let fr_ref = unsafe { &*fr };
    if fr_ref.fr_layout == FR_LEAF {
        // SAFETY: forwarded from this function's own safety doc.
        return unsafe { ses_do_win(fr_ref.fr_win) };
    }
    let mut frc = fr_ref.fr_child;
    while !frc.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { ses_do_frame(frc) } {
            return true;
        }
        // SAFETY: forwarded from this function's own safety doc.
        frc = unsafe { &*frc }.fr_next;
    }
    false
}

/// Find the first frame, starting at `fr` and walking sibling frames
/// via `fr_next`, that has a window worth saving in the session - or
/// null if none do (`ses_skipframe`).
///
/// # Safety
/// `fr`, if non-null, must be a valid pointer to a live `FrameT`, and
/// so must every frame reachable via its own `fr_next`/`fr_child`
/// chain.
#[must_use]
pub unsafe fn ses_skipframe(mut fr: *mut FrameT) -> *mut FrameT {
    while !fr.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { ses_do_frame(fr) } {
            break;
        }
        // SAFETY: forwarded from this function's own safety doc.
        fr = unsafe { &*fr }.fr_next;
    }
    fr
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer_defs::BufT;

    fn set_ssop_flags(value: u32) -> u32 {
        // SAFETY: caller holds `global_state_test_lock()` for the
        // whole duration this value matters.
        let cell = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let old = cell.ssop_flags;
        cell.ssop_flags = value;
        old
    }

    fn buf_with_bt(bt: Option<&[u8]>) -> BufT {
        BufT { b_p_bt: bt.map(<[u8]>::to_vec), ..Default::default() }
    }

    #[test]
    fn ses_do_win_excludes_a_floating_window() {
        let buf = buf_with_bt(None);
        let win = WinT { w_floating: true, w_buffer: &buf as *const BufT as *mut BufT, ..Default::default() };
        assert!(!unsafe { ses_do_win(&win as *const WinT) });
    }

    #[test]
    fn ses_do_win_blank_buffer_gated_by_ssop_blank_flag() {
        let _lock = crate::globals::global_state_test_lock();
        let old = set_ssop_flags(0);

        // No file name at all - the original's own "b_fname == NULL"
        // disjunct.
        let buf = buf_with_bt(None);
        let win = WinT { w_buffer: &buf as *const BufT as *mut BufT, ..Default::default() };
        assert!(!unsafe { ses_do_win(&win as *const WinT) });

        set_ssop_flags(opt_ssop_flag::BLANK);
        assert!(unsafe { ses_do_win(&win as *const WinT) });

        set_ssop_flags(old);
    }

    #[test]
    fn ses_do_win_nofile_buftype_with_a_name_is_also_gated_by_blank() {
        let _lock = crate::globals::global_state_test_lock();
        let old = set_ssop_flags(0);

        // Has a name, but 'buftype' is "nofile" and no terminal - the
        // original's own second disjunct.
        let mut buf = buf_with_bt(Some(b"nofile"));
        buf.b_fname = Some(b"[No Name]".to_vec());
        let win = WinT { w_buffer: &buf as *const BufT as *mut BufT, ..Default::default() };
        assert!(!unsafe { ses_do_win(&win as *const WinT) });

        set_ssop_flags(opt_ssop_flag::BLANK);
        assert!(unsafe { ses_do_win(&win as *const WinT) });

        set_ssop_flags(old);
    }

    #[test]
    fn ses_do_win_help_buffer_gated_by_ssop_help_flag() {
        let _lock = crate::globals::global_state_test_lock();
        let old = set_ssop_flags(0);

        let mut buf = buf_with_bt(Some(b"help"));
        buf.b_fname = Some(b"help.txt".to_vec());
        buf.b_help = true;
        let win = WinT { w_buffer: &buf as *const BufT as *mut BufT, ..Default::default() };
        assert!(!unsafe { ses_do_win(&win as *const WinT) });

        set_ssop_flags(opt_ssop_flag::HELP);
        assert!(unsafe { ses_do_win(&win as *const WinT) });

        set_ssop_flags(old);
    }

    #[test]
    fn ses_do_win_terminal_buffer_gated_by_ssop_terminal_flag() {
        let _lock = crate::globals::global_state_test_lock();
        let old = set_ssop_flags(0);

        let mut buf = buf_with_bt(Some(b"terminal"));
        buf.b_fname = Some(b"term://foo".to_vec());
        let win = WinT { w_buffer: &buf as *const BufT as *mut BufT, ..Default::default() };
        assert!(!unsafe { ses_do_win(&win as *const WinT) });

        set_ssop_flags(opt_ssop_flag::TERMINAL);
        assert!(unsafe { ses_do_win(&win as *const WinT) });

        set_ssop_flags(old);
    }

    #[test]
    fn ses_do_win_ordinary_file_buffer_is_always_included() {
        let _lock = crate::globals::global_state_test_lock();
        let old = set_ssop_flags(0);

        let mut buf = buf_with_bt(None);
        buf.b_fname = Some(b"real_file.rs".to_vec());
        let win = WinT { w_buffer: &buf as *const BufT as *mut BufT, ..Default::default() };
        assert!(unsafe { ses_do_win(&win as *const WinT) });

        set_ssop_flags(old);
    }

    #[test]
    fn ses_do_frame_leaf_delegates_to_ses_do_win() {
        let _lock = crate::globals::global_state_test_lock();
        let old = set_ssop_flags(0);

        let buf = buf_with_bt(None);
        let win = WinT { w_buffer: &buf as *const BufT as *mut BufT, ..Default::default() };
        let leaf = FrameT { fr_layout: FR_LEAF, fr_win: &win as *const WinT as *mut WinT, ..Default::default() };
        // Blank buffer, ssop_flags empty -> excluded, matching
        // ses_do_win's own result directly.
        assert!(!unsafe { ses_do_frame(&leaf as *const FrameT) });

        set_ssop_flags(old);
    }

    #[test]
    fn ses_do_frame_row_recurses_into_children_and_finds_a_match() {
        let _lock = crate::globals::global_state_test_lock();
        let old = set_ssop_flags(0);

        let mut blank_buf = buf_with_bt(None);
        let mut real_buf = buf_with_bt(None);
        real_buf.b_fname = Some(b"real.rs".to_vec());
        let blank_win = WinT { w_buffer: &mut blank_buf as *mut BufT, ..Default::default() };
        let real_win = WinT { w_buffer: &mut real_buf as *mut BufT, ..Default::default() };
        let mut leaf2 = FrameT { fr_layout: FR_LEAF, fr_win: &real_win as *const WinT as *mut WinT, ..Default::default() };
        let leaf1 = FrameT {
            fr_layout: FR_LEAF,
            fr_win: &blank_win as *const WinT as *mut WinT,
            fr_next: &mut leaf2 as *mut FrameT,
            ..Default::default()
        };
        let row = FrameT { fr_layout: crate::buffer_defs::FR_ROW, fr_child: &leaf1 as *const FrameT as *mut FrameT, ..Default::default() };

        // The first child (blank buffer) is excluded, but the second
        // (real file) is always included - the row as a whole must
        // report `true`.
        assert!(unsafe { ses_do_frame(&row as *const FrameT) });

        set_ssop_flags(old);
    }

    #[test]
    fn ses_skipframe_finds_the_first_matching_sibling() {
        let _lock = crate::globals::global_state_test_lock();
        let old = set_ssop_flags(0);

        let mut blank_buf = buf_with_bt(None);
        let mut real_buf = buf_with_bt(None);
        real_buf.b_fname = Some(b"real.rs".to_vec());
        let blank_win = WinT { w_buffer: &mut blank_buf as *mut BufT, ..Default::default() };
        let real_win = WinT { w_buffer: &mut real_buf as *mut BufT, ..Default::default() };
        let mut leaf2 = FrameT { fr_layout: FR_LEAF, fr_win: &real_win as *const WinT as *mut WinT, ..Default::default() };
        let mut leaf1 = FrameT {
            fr_layout: FR_LEAF,
            fr_win: &blank_win as *const WinT as *mut WinT,
            fr_next: &mut leaf2 as *mut FrameT,
            ..Default::default()
        };

        let found = unsafe { ses_skipframe(&mut leaf1 as *mut FrameT) };
        assert_eq!(found, &mut leaf2 as *mut FrameT);

        set_ssop_flags(old);
    }

    #[test]
    fn ses_skipframe_returns_null_when_no_sibling_matches() {
        let _lock = crate::globals::global_state_test_lock();
        let old = set_ssop_flags(0);

        let blank_buf = buf_with_bt(None);
        let blank_win = WinT { w_buffer: &blank_buf as *const BufT as *mut BufT, ..Default::default() };
        let mut leaf = FrameT { fr_layout: FR_LEAF, fr_win: &blank_win as *const WinT as *mut WinT, ..Default::default() };

        assert!(unsafe { ses_skipframe(&mut leaf as *mut FrameT) }.is_null());

        set_ssop_flags(old);
    }

    #[test]
    fn ses_skipframe_null_input_is_null_output() {
        assert!(unsafe { ses_skipframe(std::ptr::null_mut()) }.is_null());
    }
}
