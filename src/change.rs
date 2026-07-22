//! Translated from `src/nvim/change.c` (partial).
//!
//! `change.c` (~2200 lines) is the buffer-modification/change-tracking
//! core (`changed`/`changed_bytes`/`changed_lines`, insert-mode byte
//! insertion, indent-preservation helpers, etc.). Re-examined after
//! `memline.c`'s write side (`ml_replace`/`ml_append`/`ml_delete`)
//! was completed later in the same session that first wrote this
//! comment - that specific blocker is gone, but `changed`/
//! `changed_internal`/`changed_common`/`changed_lines_invalidate_win`
//! (etc.) turn out to need a wide spread of OTHER not-yet-translated
//! subsystems instead: `change_warning` (`apply_autocmds`, real
//! autocmd triggers - see `undo.rs`'s own module doc for the same
//! blocker), `ml_open_file` (swap-file creation), window/fold display
//! bookkeeping (`redraw_buf_status_later`, `find_wl_entry`,
//! `invalidate_botline_win`, `buf_meta_total`), `diff_internal`/
//! `diff_update_line` (`diff.c`), and `buf_inc_changedtick` (the real
//! `b:` dict watcher machinery, eval engine/phase 5). Only the one
//! genuinely self-contained function needed by `undo.c`'s
//! `bufIsChanged` is translated here: `file_ff_differs`.
//!
//! Deferred: everything else in the file - each is its own substantial
//! undertaking blocked on subsystems not yet translated (the display
//! pipeline, the fold/diff subsystems, `autocmd.c`, the eval engine's
//! `b:` dict watchers, etc. - see above).

use crate::buffer_defs::{b_flags, BufT};

/// Return true if `'fileformat'` and/or `'fileencoding'` has a
/// different value from when editing started (`save_file_ff()`
/// called). Also true when `'endofline'` was changed and `'binary'`
/// is set, or when `'bomb'` was changed and `'binary'` is not set.
/// Also true when `'endofline'` was changed and `'fixeol'` is not set.
/// When `ignore_empty` is true, don't consider a new, empty buffer to
/// be changed (`file_ff_differs`).
///
/// # Safety
/// `buf.b_ml.ml_mfp`, if non-null, must be a valid pointer to a live
/// `MemfileT` (touched transitively via `crate::memline::ml_get_buf`).
#[must_use]
pub unsafe fn file_ff_differs(buf: &mut BufT, ignore_empty: bool) -> bool {
    // In a buffer that was never loaded the options are not valid.
    if buf.b_flags & (b_flags::BF_NEVERLOADED as i32) != 0 {
        return false;
    }
    if ignore_empty
        && buf.b_flags & (b_flags::BF_NEW as i32) != 0
        && buf.b_ml.ml_line_count == 1
        // SAFETY: forwarded from this function's own safety doc.
        && unsafe { crate::memline::ml_get_buf(buf, 1) }.first() == Some(&0)
    {
        return false;
    }
    let ff_first_byte = buf.b_p_ff.as_deref().and_then(<[u8]>::first).copied().unwrap_or(0);
    if buf.b_start_ffc != i32::from(ff_first_byte) {
        return true;
    }
    if (buf.b_p_bin != 0 || buf.b_p_fixeol == 0)
        && (buf.b_start_eof != buf.b_p_eof || buf.b_start_eol != buf.b_p_eol)
    {
        return true;
    }
    if buf.b_p_bin == 0 && buf.b_start_bomb != buf.b_p_bomb {
        return true;
    }
    let Some(start_fenc) = &buf.b_start_fenc else {
        return buf.b_p_fenc.as_deref().is_some_and(|s| !s.is_empty());
    };
    buf.b_p_fenc.as_deref().unwrap_or(b"") != start_fenc.as_slice()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_ff_differs_false_for_never_loaded_buffer() {
        let mut buf = BufT { b_flags: b_flags::BF_NEVERLOADED as i32, ..Default::default() };
        assert!(!unsafe { file_ff_differs(&mut buf, false) });
    }

    #[test]
    fn file_ff_differs_false_for_new_empty_buffer_when_ignoring_empty() {
        // ml_open touches shared GLOBALS.got_int internally via
        // mf_sync - must hold the lock like every other GlobalCell-
        // touching test (see memfile.rs's mf_sync tests for the same
        // reasoning).
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        buf.b_flags = b_flags::BF_NEW as i32;

        assert!(!unsafe { file_ff_differs(&mut buf, true) });

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn file_ff_differs_true_when_new_empty_buffer_not_ignored() {
        // See file_ff_differs_false_for_new_empty_buffer_when_ignoring_
        // empty's own comment: ml_open touches shared GLOBALS.got_int.
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        buf.b_flags = b_flags::BF_NEW as i32;
        // b_start_ffc defaults to 0, which differs from b_p_ff's
        // (also-defaulted) empty/None first byte only if we force a
        // mismatch - set b_p_ff so the ffc check itself trips.
        buf.b_p_ff = Some(b"unix".to_vec());
        buf.b_start_ffc = i32::from(b'd'); // "dos", deliberately different

        assert!(unsafe { file_ff_differs(&mut buf, false) });

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn file_ff_differs_true_when_fileformat_first_char_changed() {
        let mut buf = BufT {
            b_p_ff: Some(b"dos".to_vec()),
            b_start_ffc: i32::from(b'u'), // was "unix" when editing started
            ..Default::default()
        };
        assert!(unsafe { file_ff_differs(&mut buf, false) });
    }

    #[test]
    fn file_ff_differs_false_when_nothing_changed() {
        let mut buf = BufT {
            b_p_ff: Some(b"unix".to_vec()),
            b_start_ffc: i32::from(b'u'),
            b_p_fenc: Some(b"utf-8".to_vec()),
            b_start_fenc: Some(b"utf-8".to_vec()),
            ..Default::default()
        };
        assert!(!unsafe { file_ff_differs(&mut buf, false) });
    }

    #[test]
    fn file_ff_differs_true_when_fileencoding_changed() {
        let mut buf = BufT {
            b_p_ff: Some(b"unix".to_vec()),
            b_start_ffc: i32::from(b'u'),
            b_p_fenc: Some(b"latin1".to_vec()),
            b_start_fenc: Some(b"utf-8".to_vec()),
            ..Default::default()
        };
        assert!(unsafe { file_ff_differs(&mut buf, false) });
    }

    #[test]
    fn file_ff_differs_true_when_bomb_changed_and_not_binary() {
        let mut buf = BufT {
            b_p_ff: Some(b"unix".to_vec()),
            b_start_ffc: i32::from(b'u'),
            b_p_bin: 0,
            b_p_bomb: 1,
            b_start_bomb: 0,
            ..Default::default()
        };
        assert!(unsafe { file_ff_differs(&mut buf, false) });
    }

    #[test]
    fn file_ff_differs_ignores_bomb_change_when_binary() {
        let mut buf = BufT {
            b_p_ff: Some(b"unix".to_vec()),
            b_start_ffc: i32::from(b'u'),
            b_p_bin: 1,
            b_p_bomb: 1,
            b_start_bomb: 0,
            ..Default::default()
        };
        assert!(!unsafe { file_ff_differs(&mut buf, false) });
    }
}
