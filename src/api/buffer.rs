//! Translated from `src/nvim/api/buffer.c` (tractable subset only -
//! most of this file needs `Arena`/`Object`-conversion machinery,
//! `Dict`-scoped-variable access, keymap/mark manipulation, and
//! `rename_buffer`/`do_buffer`, none of which are wired into this API
//! layer yet).
//!
//! Translated: [`nvim_buf_get_lines`], [`nvim_buf_line_count`], [`nvim_buf_get_changedtick`],
//! [`nvim_buf_get_mark`], [`nvim_buf_get_name`], [`nvim_buf_is_loaded`],
//! [`nvim_buf_is_valid`] - every one of these is a thin, real
//! `find_buffer_by_handle` + one-field read, with no other
//! dependency.
//!
//! Deferred (each needs a real, not-yet-translated subsystem beyond
//! `find_buffer_by_handle` itself): `nvim_buf_attach`/`nvim_buf_detach`
//! (channel/Lua-callback registration), `nvim_buf_set_lines`/
//! `nvim_buf_set_text` (real buffer-line mutation, `extmark_splice_cols`/
//! `mark_adjust`), `nvim_buf_get_offset` (needs `ml_find_line`'s byte-
//! offset accumulation across the whole buffer), `nvim_buf_get/set/
//! del_var` (`dict_get_value`/`dict_set_var`, the API layer's
//! `Object`-to-`typval_T` bridge), `nvim_buf_get_keymap`/`set_keymap`/
//! `del_keymap` (the mapping subsystem), `nvim_buf_set_name` (needs
//! `rename_buffer`/`ctx_switch`), `nvim_buf_delete` (needs `do_buffer`),
//! `nvim_buf_del_mark`/`nvim_buf_set_mark` (needs mark-setting
//! machinery), `nvim_buf_call` (needs the
//! Lua host).

use crate::api::private::defs::{
    Array, Boolean, Buffer, Error, ErrorType, Integer, NvimString, Object, StringArray,
};
use crate::api::private::helpers::find_buffer_by_handle;

/// Return buffer lines from the 0-based, end-exclusive range
/// `[start, end)` (`nvim_buf_get_lines`).
///
/// # Safety
/// Forwarded from [`find_buffer_by_handle`] and
/// [`crate::memline::ml_get_buf`].
pub unsafe fn nvim_buf_get_lines(
    channel_id: u64,
    buf: Buffer,
    start: Integer,
    end: Integer,
    strict_indexing: Boolean,
    err: &mut Error,
) -> StringArray {
    let b = unsafe { find_buffer_by_handle(buf, err) };
    if b.is_null() || unsafe { (*b).b_ml.ml_mfp }.is_null() {
        return Vec::new();
    }

    let mut oob = false;
    let start = crate::api::private::helpers::normalize_index(
        unsafe { &*b },
        start,
        true,
        &mut oob,
    );
    let end =
        crate::api::private::helpers::normalize_index(unsafe { &*b }, end, true, &mut oob);
    if strict_indexing && oob {
        err.r#type = ErrorType::Validation;
        err.msg = Some("Index out of bounds".to_string());
        return Vec::new();
    }

    let count = end.saturating_sub(start);
    let replace_nl = channel_id != crate::api::private::defs::VIML_INTERNAL_CALL;
    let mut lines = Vec::with_capacity(count as usize);
    for offset in 0..count {
        let lnum = (start + offset) as crate::pos_defs::LinenrT;
        let line = unsafe { crate::memline::ml_get_buf(&mut *b, lnum) };
        let len = unsafe { crate::memline::ml_get_buf_len(&mut *b, lnum) } as usize;
        let mut line = line[..len].to_vec();
        if replace_nl {
            for byte in &mut line {
                if *byte == b'\n' {
                    *byte = 0;
                }
            }
        }
        lines.push(line);
    }
    lines
}

/// Get the number of lines in buffer `buf` (`0` for the current
/// buffer), or `0` on failure/if the buffer is unloaded
/// (`nvim_buf_line_count`).
///
/// # Safety
/// Forwarded from [`find_buffer_by_handle`]'s own safety doc.
pub unsafe fn nvim_buf_line_count(buf: Buffer, err: &mut Error) -> Integer {
    // SAFETY: forwarded from this function's own safety doc.
    let b = unsafe { find_buffer_by_handle(buf, err) };
    if b.is_null() {
        return 0;
    }
    // SAFETY: `b` is non-null per the check above.
    if unsafe { (*b).b_ml.ml_mfp }.is_null() {
        return 0;
    }
    // SAFETY: `b` is non-null per the check above.
    i64::from(unsafe { (*b).b_ml.ml_line_count })
}

/// Get `b:changedtick` for buffer `buf` (`0` for the current buffer),
/// or `-1` on failure (`nvim_buf_get_changedtick`).
///
/// # Safety
/// Forwarded from [`find_buffer_by_handle`]'s own safety doc.
pub unsafe fn nvim_buf_get_changedtick(buf: Buffer, err: &mut Error) -> Integer {
    // SAFETY: forwarded from this function's own safety doc.
    let b = unsafe { find_buffer_by_handle(buf, err) };
    if b.is_null() {
        return -1;
    }
    // SAFETY: `b` is non-null per the check above.
    crate::buffer::buf_get_changedtick(unsafe { &*b })
}

/// Return the `(row, column)` position of named mark `name`
/// (`nvim_buf_get_mark`).
///
/// # Safety
/// Forwarded from [`find_buffer_by_handle`] and
/// [`crate::mark::mark_get`]; `GLOBALS.curwin` must point to a live
/// window.
pub unsafe fn nvim_buf_get_mark(buf: Buffer, name: &NvimString, err: &mut Error) -> Array {
    let b = unsafe { find_buffer_by_handle(buf, err) };
    if b.is_null() {
        return Vec::new();
    }
    if name.len() != 1 {
        err.r#type = ErrorType::Validation;
        err.msg = Some(if name.is_empty() {
            "Invalid mark name (must be a single char)".to_string()
        } else {
            format!(
                "Invalid mark name (must be a single char): '{}'",
                String::from_utf8_lossy(name)
            )
        });
        return Vec::new();
    }

    let mark = unsafe {
        crate::mark::mark_get(
            &mut *b,
            crate::globals::GLOBALS.get_mut().curwin,
            None,
            crate::mark_defs::MarkGet::AllNoResolve,
            i32::from(name[0]),
        )
    };
    if mark.is_null() {
        err.r#type = ErrorType::Validation;
        err.msg = Some(format!("Invalid mark name: '{}'", String::from_utf8_lossy(name)));
        return Vec::new();
    }

    let pos = if unsafe { (*mark).fnum } == unsafe { (*b).handle } {
        unsafe { (*mark).mark }
    } else {
        crate::pos_defs::PosT::default()
    };
    vec![Object::Integer(i64::from(pos.lnum)), Object::Integer(i64::from(pos.col))]
}

/// Get the full/absolute filepath of buffer `buf` (`0` for the
/// current buffer), or an empty string on failure/if the buffer has
/// no file name (`nvim_buf_get_name`).
///
/// # Safety
/// Forwarded from [`find_buffer_by_handle`]'s own safety doc.
pub unsafe fn nvim_buf_get_name(buf: Buffer, err: &mut Error) -> NvimString {
    // SAFETY: forwarded from this function's own safety doc.
    let b = unsafe { find_buffer_by_handle(buf, err) };
    if b.is_null() {
        return NvimString::default();
    }
    // SAFETY: `b` is non-null per the check above.
    match unsafe { &(*b).b_ffname } {
        Some(name) => name.clone(),
        None => NvimString::default(),
    }
}

/// Whether buffer `buf` (`0` for the current buffer) is both valid
/// and loaded (`nvim_buf_is_loaded`). Like the original, a failed
/// lookup is not itself an error - it simply means `buf` is invalid,
/// matching the original's own `stub`-`Error`-then-discard pattern.
///
/// # Safety
/// Forwarded from [`find_buffer_by_handle`]'s own safety doc.
#[must_use]
pub unsafe fn nvim_buf_is_loaded(buf: Buffer) -> Boolean {
    let mut stub = Error::default();
    // SAFETY: forwarded from this function's own safety doc.
    let b = unsafe { find_buffer_by_handle(buf, &mut stub) };
    if b.is_null() {
        return false;
    }
    // SAFETY: `b` is non-null per the check above.
    !unsafe { (*b).b_ml.ml_mfp }.is_null()
}

/// Whether `buf` (`0` for the current buffer) refers to a currently
/// valid buffer (`nvim_buf_is_valid`).
///
/// # Safety
/// Forwarded from [`find_buffer_by_handle`]'s own safety doc.
#[must_use]
pub unsafe fn nvim_buf_is_valid(buf: Buffer) -> Boolean {
    let mut stub = Error::default();
    // SAFETY: forwarded from this function's own safety doc.
    !unsafe { find_buffer_by_handle(buf, &mut stub) }.is_null()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer_defs::{BufT, WinT};
    use crate::memfile_defs::{MemfileT, MfdirtyT};

    fn test_memfile() -> MemfileT {
        MemfileT {
            mf_fname: None,
            mf_ffname: None,
            mf_fd: None,
            mf_flags: 0,
            mf_reopen: false,
            mf_free_first: std::ptr::null_mut(),
            mf_hash: crate::map::Map::default(),
            mf_trans: crate::map::Map::default(),
            mf_blocknr_max: 0,
            mf_blocknr_min: -1,
            mf_neg_count: 0,
            mf_infile_count: 0,
            mf_page_size: 4096,
            mf_dirty: MfdirtyT::No,
        }
    }

    struct BufFixture {
        buf_ptr: *mut BufT,
        win_ptr: *mut WinT,
        prev_lastbuf: *mut BufT,
        prev_curbuf: *mut BufT,
        prev_curwin: *mut WinT,
    }

    impl BufFixture {
        fn new(handle: crate::types_defs::HandleT) -> Self {
            // `Box::into_raw` (not a live `Box` field alongside a
            // separately-derived raw pointer) is essential here:
            // keeping a live `Box<BufT>` around WHILE ALSO writing
            // through a raw pointer derived from it is a genuine Tree
            // Borrows violation (confirmed via `cargo miri test` on
            // an earlier draft - the Box's own Drop-time internal
            // reborrow conflicts with any sibling raw-pointer write
            // that happened after construction). Converting fully to
            // a raw pointer up front, then manually reconstructing
            // the `Box` only once, in `Drop`, avoids this entirely.
            let buf_ptr = Box::into_raw(Box::new(BufT { handle, ..Default::default() }));
            let win_ptr =
                Box::into_raw(Box::new(WinT { w_buffer: buf_ptr, ..Default::default() }));

            // SAFETY: single-threaded test, GLOBALS restored in Drop.
            unsafe {
                let g = crate::globals::GLOBALS.get_mut();
                let prev_lastbuf = g.lastbuf;
                let prev_curbuf = g.curbuf;
                let prev_curwin = g.curwin;
                g.lastbuf = buf_ptr;
                g.curbuf = buf_ptr;
                g.curwin = win_ptr;
                BufFixture { buf_ptr, win_ptr, prev_lastbuf, prev_curbuf, prev_curwin }
            }
        }

        fn buf_mut(&mut self) -> &mut BufT {
            // SAFETY: `buf_ptr` was allocated in `new` and stays
            // valid until this fixture's own `Drop` runs.
            unsafe { &mut *self.buf_ptr }
        }

        fn handle(&self) -> crate::types_defs::HandleT {
            // SAFETY: same as `buf_mut`'s own doc.
            unsafe { (*self.buf_ptr).handle }
        }
    }

    impl Drop for BufFixture {
        fn drop(&mut self) {
            // SAFETY: restoring exactly what `new` overwrote, then
            // reclaiming the `Box` allocated via `Box::into_raw` in
            // `new` - the only reconstruction of a `Box` over this
            // pointer, so there is no sibling-reborrow conflict.
            unsafe {
                let g = crate::globals::GLOBALS.get_mut();
                g.lastbuf = self.prev_lastbuf;
                g.curbuf = self.prev_curbuf;
                g.curwin = self.prev_curwin;
                drop(Box::from_raw(self.win_ptr));
                drop(Box::from_raw(self.buf_ptr));
            }
        }

    }

    unsafe fn close_memline(fx: &mut BufFixture) {
        let mfp = fx.buf_mut().b_ml.ml_mfp;
        if !mfp.is_null() {
            fx.buf_mut().b_ml.ml_mfp = std::ptr::null_mut();
            unsafe { crate::memfile::mf_close(*Box::from_raw(mfp), false) };
        }
    }

    #[test]
    fn nvim_buf_line_count_zero_for_an_unloaded_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let fx = BufFixture::new(1);
        let handle = fx.handle();
        let mut err = Error::default();
        // SAFETY: `fx` sets up a valid GLOBALS.lastbuf/curbuf; the
        // buffer's own b_ml.ml_mfp is null (unloaded) by default.
        let count = unsafe { nvim_buf_line_count(handle, &mut err) };
        assert_eq!(count, 0);
        assert!(!err.is_set());
    }

    #[test]
    fn nvim_buf_get_lines_returns_the_requested_range() {
        let _lock = crate::globals::global_state_test_lock();
        let mut fx = BufFixture::new(44);
        assert_eq!(unsafe { crate::memline::ml_open(fx.buf_mut()) }, crate::vim_defs::OK);
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(fx.buf_mut(), 1, b"one\0") },
            crate::vim_defs::OK
        );
        assert_eq!(
            unsafe { crate::memline::ml_append_buf(fx.buf_mut(), 1, b"two\0", 4, false) },
            crate::vim_defs::OK
        );
        let mut err = Error::default();

        let lines = unsafe { nvim_buf_get_lines(0, fx.handle(), 0, -1, true, &mut err) };

        assert!(!err.is_set());
        assert_eq!(lines, vec![b"one".to_vec(), b"two".to_vec()]);
        unsafe { close_memline(&mut fx) };
    }

    #[test]
    fn nvim_buf_get_lines_strict_indexing_rejects_out_of_bounds_ranges() {
        let _lock = crate::globals::global_state_test_lock();
        let mut fx = BufFixture::new(45);
        assert_eq!(unsafe { crate::memline::ml_open(fx.buf_mut()) }, crate::vim_defs::OK);
        let mut err = Error::default();
        assert!(
            unsafe { nvim_buf_get_lines(0, fx.handle(), 0, 99, true, &mut err) }.is_empty()
        );
        assert_eq!(err.msg.as_deref(), Some("Index out of bounds"));
        unsafe { close_memline(&mut fx) };
    }

    #[test]
    fn nvim_buf_get_lines_returns_empty_for_an_unloaded_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let fx = BufFixture::new(46);
        let mut err = Error::default();
        assert!(
            unsafe { nvim_buf_get_lines(0, fx.handle(), 0, -1, true, &mut err) }.is_empty()
        );
        assert!(!err.is_set());
    }

    #[test]
    fn nvim_buf_line_count_returns_the_real_line_count_when_loaded() {
        let _lock = crate::globals::global_state_test_lock();
        let mut fx = BufFixture::new(2);
        fx.buf_mut().b_ml.ml_line_count = 17;
        let mut mfp = Box::new(test_memfile());
        fx.buf_mut().b_ml.ml_mfp = std::ptr::addr_of_mut!(*mfp);
        let handle = fx.handle();

        let mut err = Error::default();
        // SAFETY: `fx` sets up a valid GLOBALS.lastbuf/curbuf, and
        // `mfp` outlives this call.
        let count = unsafe { nvim_buf_line_count(handle, &mut err) };
        assert_eq!(count, 17);
        assert!(!err.is_set());

        fx.buf_mut().b_ml.ml_mfp = std::ptr::null_mut();
    }

    #[test]
    fn nvim_buf_get_changedtick_delegates_to_buf_get_changedtick() {
        let _lock = crate::globals::global_state_test_lock();
        let fx = BufFixture::new(3);
        let handle = fx.handle();
        let mut err = Error::default();
        // SAFETY: `fx` sets up a valid GLOBALS.lastbuf/curbuf.
        let tick = unsafe { nvim_buf_get_changedtick(handle, &mut err) };
        // A freshly-constructed BufT's own changedtick_di starts at 0,
        // matching buf_get_changedtick's own already-tested default.
        assert_eq!(tick, 0);
        assert!(!err.is_set());
    }

    #[test]
    fn nvim_buf_get_changedtick_minus_1_for_an_unknown_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let _fx = BufFixture::new(4);
        let mut err = Error::default();
        // SAFETY: `_fx` sets up a valid GLOBALS.lastbuf/curbuf, but
        // `9999` is a genuinely unrecognized handle.
        let tick = unsafe { nvim_buf_get_changedtick(9999, &mut err) };
        assert_eq!(tick, -1);
        assert!(err.is_set());
    }

    #[test]
    fn nvim_buf_get_mark_returns_the_local_mark_position() {
        let _lock = crate::globals::global_state_test_lock();
        let mut fx = BufFixture::new(41);
        fx.buf_mut().b_namedm[0].mark = crate::pos_defs::PosT {
            lnum: 7,
            col: 3,
            coladd: 0,
        };
        fx.buf_mut().b_namedm[0].fnum = 41;
        let mut err = Error::default();

        let position =
            unsafe { nvim_buf_get_mark(fx.handle(), &b"a".to_vec(), &mut err) };

        assert!(!err.is_set());
        assert!(matches!(
            position.as_slice(),
            [Object::Integer(7), Object::Integer(3)]
        ));
    }

    #[test]
    fn nvim_buf_get_mark_rejects_a_multibyte_name() {
        let _lock = crate::globals::global_state_test_lock();
        let fx = BufFixture::new(42);
        let mut err = Error::default();
        assert!(
            unsafe { nvim_buf_get_mark(fx.handle(), &b"ab".to_vec(), &mut err) }.is_empty()
        );
        assert_eq!(
            err.msg.as_deref(),
            Some("Invalid mark name (must be a single char): 'ab'")
        );
    }

    #[test]
    fn nvim_buf_get_mark_rejects_an_invalid_single_character_name() {
        let _lock = crate::globals::global_state_test_lock();
        let fx = BufFixture::new(43);
        let mut err = Error::default();
        assert!(
            unsafe { nvim_buf_get_mark(fx.handle(), &b"~".to_vec(), &mut err) }.is_empty()
        );
        assert_eq!(err.msg.as_deref(), Some("Invalid mark name: '~'"));
    }

    #[test]
    fn nvim_buf_get_name_returns_the_real_filename() {
        let _lock = crate::globals::global_state_test_lock();
        let mut fx = BufFixture::new(5);
        fx.buf_mut().b_ffname = Some(b"foo.txt".to_vec());
        let handle = fx.handle();
        let mut err = Error::default();
        // SAFETY: `fx` sets up a valid GLOBALS.lastbuf/curbuf.
        let name = unsafe { nvim_buf_get_name(handle, &mut err) };
        assert_eq!(name, b"foo.txt");
        assert!(!err.is_set());
    }

    #[test]
    fn nvim_buf_get_name_empty_when_no_filename() {
        let _lock = crate::globals::global_state_test_lock();
        let fx = BufFixture::new(6);
        let handle = fx.handle();
        let mut err = Error::default();
        // SAFETY: `fx` sets up a valid GLOBALS.lastbuf/curbuf; the
        // buffer's own b_ffname is None by default.
        let name = unsafe { nvim_buf_get_name(handle, &mut err) };
        assert!(name.is_empty());
        assert!(!err.is_set());
    }

    #[test]
    fn nvim_buf_is_loaded_and_is_valid_true_for_a_loaded_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let mut fx = BufFixture::new(7);
        let mut mfp = Box::new(test_memfile());
        fx.buf_mut().b_ml.ml_mfp = std::ptr::addr_of_mut!(*mfp);
        let handle = fx.handle();

        // SAFETY: `fx` sets up a valid GLOBALS.lastbuf/curbuf, and
        // `mfp` outlives this call.
        assert!(unsafe { nvim_buf_is_loaded(handle) });
        // SAFETY: same as above.
        assert!(unsafe { nvim_buf_is_valid(handle) });

        fx.buf_mut().b_ml.ml_mfp = std::ptr::null_mut();
    }

    #[test]
    fn nvim_buf_is_loaded_false_for_an_unloaded_but_valid_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let fx = BufFixture::new(8);
        let handle = fx.handle();
        // SAFETY: `fx` sets up a valid GLOBALS.lastbuf/curbuf; the
        // buffer's own b_ml.ml_mfp is null (unloaded) by default.
        assert!(!unsafe { nvim_buf_is_loaded(handle) });
        // SAFETY: same as above.
        assert!(unsafe { nvim_buf_is_valid(handle) });
    }

    #[test]
    fn nvim_buf_is_valid_false_for_an_unknown_handle() {
        let _lock = crate::globals::global_state_test_lock();
        let _fx = BufFixture::new(9);
        // SAFETY: `_fx` sets up a valid GLOBALS.lastbuf/curbuf, but
        // `9999` is a genuinely unrecognized handle.
        assert!(!unsafe { nvim_buf_is_valid(9999) });
    }
}
