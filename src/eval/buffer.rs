//! Translated from `src/nvim/eval/buffer.c` (tractable core only).
//!
//! `eval/buffer.c` (~830 lines) implements buffer-related Vimscript
//! builtins: `bufadd()`, `bufname()`, `bufnr()`, `getbufline()`,
//! `setbufline()`, `deletebufline()`, `getbufinfo()`, and more. Most
//! need substantial not-yet-translated machinery (`buflist_new`'s
//! full buffer-creation path, `ml_get`/`ml_replace`'s line-level
//! mutation with undo tracking, `buflist_findname_exp`'s full-path/
//! wildcard buffer-name matching) - this file starts with the
//! smallest, most self-contained subset: resolving a `{buf}` argument
//! (number or a handful of special strings) to a real buffer.
//!
//! Translated: `tv_get_buf` (`eval/funcs.c`'s own `tv_get_buf`,
//! hosted here instead since every OTHER caller of it lives in this
//! file) - only its own tractable dispatch is modeled: `Number` (via
//! the already-existing [`crate::buffer::buflist_findnr`]), empty
//! `String` (current buffer), `"$"` (last buffer, via
//! `GLOBALS.lastbuf`), and the single-character `"%"`/`"#"` special
//! buffer references (current/alternate buffer, matching
//! `buflist_findpat`'s own fast path for these two - see its doc
//! comment there). Any OTHER, non-empty string needs
//! `buflist_findpat`'s general pattern-matching search (itself needing
//! `file_pat_to_reg_pat` plus a real Vimscript-pattern-matching regex
//! engine, neither translated) and panics via `unimplemented!()` if
//! actually reached.
//!
//! Also translated, all built directly on `tv_get_buf`:
//! `bufexists()`, `buflisted()`, `bufloaded()`, `bufname()`, `bufnr()`
//! (its own `{create}` second-argument path, which needs
//! `buflist_new`, is NOT modeled and panics via `unimplemented!()` if
//! actually reached - matching this module's own established
//! "translate the common path, panic loudly on a genuinely
//! untranslated-but-reached path" convention).

use crate::eval::typval_defs::{TypvalT, TypvalValue};

/// Resolve a `{buf}` argument to a buffer pointer (`tv_get_buf`,
/// `eval/funcs.c`). Only the common, tractable cases are modeled:
/// - `Number`: via the already-existing
///   [`crate::buffer::buflist_findnr`].
/// - Empty `String` (or no string at all - matching the original's
///   own `*name == NUL` check after a plain, non-numeric,
///   non-`Number` typval already stringifies to `""`): the current
///   buffer, `GLOBALS.curbuf`.
/// - `"$"`: the last buffer, `GLOBALS.lastbuf`.
/// - `"%"`/`"#"`: the current/alternate buffer (`buflist_findpat`'s
///   own single-character fast path - see its own C comment), via
///   [`crate::buffer::buflist_findnr`] on `GLOBALS.curbuf`'s own
///   handle / `GLOBALS.curwin.w_alt_fnum` respectively.
///
/// Any OTHER non-empty string needs `buflist_findpat`'s general
/// pattern-matching search (not yet translated - needs
/// `file_pat_to_reg_pat` plus a real Vimscript-pattern regex engine)
/// and panics via `unimplemented!()` if actually reached.
///
/// # Safety
/// Touches `crate::globals::GLOBALS`; forwards
/// [`crate::buffer::buflist_findnr`]'s own safety doc.
#[must_use]
pub(crate) unsafe fn tv_get_buf(tv: &TypvalT) -> *mut crate::buffer_defs::BufT {
    match &tv.value {
        TypvalValue::Number(n) => {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::buffer::buflist_findnr(*n as i32) }
        }
        _ => {
            let name = crate::eval::typval::tv_get_string(tv);
            if name.is_empty() {
                // SAFETY: forwarded from this function's own safety doc.
                return unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
            }
            if name == b"$" {
                // SAFETY: forwarded from this function's own safety doc.
                return unsafe { crate::globals::GLOBALS.get_mut() }.lastbuf;
            }
            if name == b"%" {
                // SAFETY: forwarded from this function's own safety doc.
                let curbuf_handle = unsafe { &*crate::globals::GLOBALS.get_mut().curbuf }.handle;
                // SAFETY: forwarded from this function's own safety doc.
                return unsafe { crate::buffer::buflist_findnr(curbuf_handle) };
            }
            if name == b"#" {
                // SAFETY: forwarded from this function's own safety doc.
                let alt_fnum = unsafe { &*crate::globals::GLOBALS.get_mut().curwin }.w_alt_fnum;
                // SAFETY: forwarded from this function's own safety doc.
                return unsafe { crate::buffer::buflist_findnr(alt_fnum) };
            }
            unimplemented!(
                "tv_get_buf: general buffer-name pattern matching needs buflist_findpat, not yet translated"
            );
        }
    }
}

/// `bufexists({expr})` - whether a buffer for `{expr}` exists
/// (`f_bufexists`, `eval/buffer.c`), via [`tv_get_buf`].
///
/// # Safety
/// Forwarded from [`tv_get_buf`]'s own safety doc.
pub(crate) unsafe fn f_bufexists(argvars: &[TypvalT], rettv: &mut TypvalT) {
    // SAFETY: forwarded from this function's own safety doc.
    let buf = unsafe { tv_get_buf(&argvars[0]) };
    rettv.value = TypvalValue::Number(i64::from(!buf.is_null()));
}

/// `buflisted({expr})` - whether the buffer for `{expr}` exists and
/// has `'buflisted'` set (`f_buflisted`, `eval/buffer.c`), via
/// [`tv_get_buf`].
///
/// # Safety
/// Forwarded from [`tv_get_buf`]'s own safety doc.
pub(crate) unsafe fn f_buflisted(argvars: &[TypvalT], rettv: &mut TypvalT) {
    // SAFETY: forwarded from this function's own safety doc.
    let buf = unsafe { tv_get_buf(&argvars[0]) };
    // SAFETY: forwarded from this function's own safety doc.
    let listed = !buf.is_null() && unsafe { &*buf }.b_p_bl != 0;
    rettv.value = TypvalValue::Number(i64::from(listed));
}

/// `bufloaded({expr})` - whether the buffer for `{expr}` exists and
/// is loaded (`f_bufloaded`, `eval/buffer.c`), via [`tv_get_buf`].
///
/// # Safety
/// Forwarded from [`tv_get_buf`]'s own safety doc.
pub(crate) unsafe fn f_bufloaded(argvars: &[TypvalT], rettv: &mut TypvalT) {
    // SAFETY: forwarded from this function's own safety doc.
    let buf = unsafe { tv_get_buf(&argvars[0]) };
    // SAFETY: forwarded from this function's own safety doc.
    let loaded = !buf.is_null() && !unsafe { &*buf }.b_ml.ml_mfp.is_null();
    rettv.value = TypvalValue::Number(i64::from(loaded));
}

/// `bufname([{expr}])` - the display name of the buffer for `{expr}`
/// (defaults to the current buffer) (`f_bufname`, `eval/buffer.c`),
/// via [`tv_get_buf`].
///
/// # Safety
/// Forwarded from [`tv_get_buf`]'s own safety doc.
pub(crate) unsafe fn f_bufname(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let buf = if argvars.is_empty() {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { tv_get_buf(&argvars[0]) }
    };
    let name = if buf.is_null() {
        None
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { &*buf }.b_fname.clone()
    };
    rettv.value = TypvalValue::String(name);
}

/// `bufnr([{expr} [, {create}]])` - the buffer number for `{expr}`
/// (defaults to the current buffer), `-1` if not found (`f_bufnr`,
/// `eval/buffer.c`), via [`tv_get_buf`]. `{create}` (create a new
/// buffer when not found) is NOT modeled - needs `buflist_new`, not
/// yet translated - and panics via `unimplemented!()` if actually
/// reached (i.e. only when the buffer genuinely isn't found AND a
/// truthy `{create}` was passed, matching the original's own
/// short-circuit order exactly).
///
/// # Safety
/// Forwarded from [`tv_get_buf`]'s own safety doc.
pub(crate) unsafe fn f_bufnr(argvars: &[TypvalT], rettv: &mut TypvalT) {
    let buf = if argvars.is_empty() {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { tv_get_buf(&argvars[0]) }
    };

    if buf.is_null() && argvars.len() > 1 && crate::eval::typval::tv_get_number(&argvars[1]) != 0 {
        unimplemented!("bufnr(): {{create}} truthy needs buflist_new, not yet translated");
    }

    let n = if buf.is_null() {
        -1
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        i64::from(unsafe { &*buf }.handle)
    };
    rettv.value = TypvalValue::Number(n);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn num(n: crate::eval::typval_defs::VarnumberT) -> TypvalT {
        TypvalT { value: TypvalValue::Number(n), ..Default::default() }
    }

    fn string(s: &[u8]) -> TypvalT {
        TypvalT { value: TypvalValue::String(Some(s.to_vec())), ..Default::default() }
    }

    /// RAII guard restoring `GLOBALS.curbuf`/`curwin`/`lastbuf` on
    /// drop - callers must hold `global_state_test_lock()` for the
    /// guard's whole lifetime.
    struct BufGlobalsGuard {
        prev_curbuf: *mut crate::buffer_defs::BufT,
        prev_curwin: *mut crate::buffer_defs::WinT,
        prev_lastbuf: *mut crate::buffer_defs::BufT,
    }

    impl BufGlobalsGuard {
        fn set(
            curbuf: *mut crate::buffer_defs::BufT,
            curwin: *mut crate::buffer_defs::WinT,
            lastbuf: *mut crate::buffer_defs::BufT,
        ) -> Self {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            let guard = BufGlobalsGuard {
                prev_curbuf: globals.curbuf,
                prev_curwin: globals.curwin,
                prev_lastbuf: globals.lastbuf,
            };
            globals.curbuf = curbuf;
            globals.curwin = curwin;
            globals.lastbuf = lastbuf;
            guard
        }
    }

    impl Drop for BufGlobalsGuard {
        fn drop(&mut self) {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            globals.curbuf = self.prev_curbuf;
            globals.curwin = self.prev_curwin;
            globals.lastbuf = self.prev_lastbuf;
        }
    }

    // ---- tv_get_buf ----

    #[test]
    fn tv_get_buf_number_finds_by_handle() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT { handle: 7, ..Default::default() };
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let mut win = crate::buffer_defs::WinT::default();
        let _guard = BufGlobalsGuard::set(std::ptr::null_mut(), &mut win, buf_ptr);

        assert_eq!(unsafe { tv_get_buf(&num(7)) }, buf_ptr);
    }

    #[test]
    fn tv_get_buf_number_0_uses_alternate_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT { handle: 3, ..Default::default() };
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let mut win = crate::buffer_defs::WinT { w_alt_fnum: 3, ..Default::default() };
        let win_ptr = &mut win as *mut crate::buffer_defs::WinT;
        let _guard = BufGlobalsGuard::set(std::ptr::null_mut(), win_ptr, buf_ptr);

        assert_eq!(unsafe { tv_get_buf(&num(0)) }, buf_ptr);
    }

    #[test]
    fn tv_get_buf_empty_string_returns_curbuf() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT::default();
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let mut win = crate::buffer_defs::WinT::default();
        let _guard = BufGlobalsGuard::set(buf_ptr, &mut win, std::ptr::null_mut());

        assert_eq!(unsafe { tv_get_buf(&string(b"")) }, buf_ptr);
    }

    #[test]
    fn tv_get_buf_dollar_returns_lastbuf() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT::default();
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let mut win = crate::buffer_defs::WinT::default();
        let _guard = BufGlobalsGuard::set(std::ptr::null_mut(), &mut win, buf_ptr);

        assert_eq!(unsafe { tv_get_buf(&string(b"$")) }, buf_ptr);
    }

    #[test]
    fn tv_get_buf_percent_returns_curbuf_via_its_own_handle() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT { handle: 5, ..Default::default() };
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let mut win = crate::buffer_defs::WinT::default();
        let _guard = BufGlobalsGuard::set(buf_ptr, &mut win, buf_ptr);

        assert_eq!(unsafe { tv_get_buf(&string(b"%")) }, buf_ptr);
    }

    #[test]
    fn tv_get_buf_hash_returns_the_alternate_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT { handle: 9, ..Default::default() };
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let mut win = crate::buffer_defs::WinT { w_alt_fnum: 9, ..Default::default() };
        let win_ptr = &mut win as *mut crate::buffer_defs::WinT;
        let _guard = BufGlobalsGuard::set(std::ptr::null_mut(), win_ptr, buf_ptr);

        assert_eq!(unsafe { tv_get_buf(&string(b"#")) }, buf_ptr);
    }

    #[test]
    fn tv_get_buf_other_string_is_unimplemented() {
        let result = std::panic::catch_unwind(|| unsafe { tv_get_buf(&string(b"some_pattern")) });
        assert!(result.is_err(), "expected a panic (buflist_findpat not yet translated)");
    }

    // ---- f_bufexists / f_buflisted / f_bufloaded ----

    #[test]
    fn bufexists_true_when_found() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT { handle: 1, ..Default::default() };
        let mut win = crate::buffer_defs::WinT::default();
        let _guard = BufGlobalsGuard::set(std::ptr::null_mut(), &mut win, &mut buf as *mut crate::buffer_defs::BufT);

        let mut rettv = TypvalT::default();
        unsafe { f_bufexists(&[num(1)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(1));
    }

    #[test]
    fn bufexists_false_when_not_found() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = crate::buffer_defs::WinT::default();
        let _guard = BufGlobalsGuard::set(std::ptr::null_mut(), &mut win, std::ptr::null_mut());

        let mut rettv = TypvalT::default();
        unsafe { f_bufexists(&[num(42)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    #[test]
    fn buflisted_reflects_b_p_bl() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT { handle: 1, b_p_bl: 1, ..Default::default() };
        let buf_ptr = &mut buf as *mut crate::buffer_defs::BufT;
        let mut win = crate::buffer_defs::WinT::default();
        let _guard = BufGlobalsGuard::set(std::ptr::null_mut(), &mut win, buf_ptr);

        let mut rettv = TypvalT::default();
        unsafe { f_buflisted(&[num(1)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(1));

        unsafe { &mut *buf_ptr }.b_p_bl = 0;
        let mut rettv = TypvalT::default();
        unsafe { f_buflisted(&[num(1)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    #[test]
    fn bufloaded_reflects_ml_mfp() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT { handle: 1, ..Default::default() };
        let mut win = crate::buffer_defs::WinT::default();
        let _guard = BufGlobalsGuard::set(std::ptr::null_mut(), &mut win, &mut buf as *mut crate::buffer_defs::BufT);

        // Default b_ml.ml_mfp is null - not loaded.
        let mut rettv = TypvalT::default();
        unsafe { f_bufloaded(&[num(1)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(0));
    }

    // ---- f_bufname / f_bufnr ----

    #[test]
    fn bufname_returns_the_current_buffer_name_with_no_args() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT { b_fname: Some(b"foo.txt".to_vec()), ..Default::default() };
        let mut win = crate::buffer_defs::WinT::default();
        let _guard = BufGlobalsGuard::set(&mut buf as *mut crate::buffer_defs::BufT, &mut win, std::ptr::null_mut());

        let mut rettv = TypvalT::default();
        unsafe { f_bufname(&[], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(Some(b"foo.txt".to_vec())));
    }

    #[test]
    fn bufname_returns_none_when_buffer_not_found() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = crate::buffer_defs::WinT::default();
        let _guard = BufGlobalsGuard::set(std::ptr::null_mut(), &mut win, std::ptr::null_mut());

        let mut rettv = TypvalT::default();
        unsafe { f_bufname(&[num(999)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::String(None));
    }

    #[test]
    fn bufnr_returns_the_current_buffer_number_with_no_args() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT { handle: 4, ..Default::default() };
        let mut win = crate::buffer_defs::WinT::default();
        let _guard = BufGlobalsGuard::set(&mut buf as *mut crate::buffer_defs::BufT, &mut win, std::ptr::null_mut());

        let mut rettv = TypvalT::default();
        unsafe { f_bufnr(&[], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(4));
    }

    #[test]
    fn bufnr_returns_minus_1_when_not_found() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = crate::buffer_defs::WinT::default();
        let _guard = BufGlobalsGuard::set(std::ptr::null_mut(), &mut win, std::ptr::null_mut());

        let mut rettv = TypvalT::default();
        unsafe { f_bufnr(&[num(999)], &mut rettv) };
        assert_eq!(rettv.value, TypvalValue::Number(-1));
    }

    #[test]
    fn bufnr_create_flag_is_unimplemented_when_not_found() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = crate::buffer_defs::WinT::default();
        let _guard = BufGlobalsGuard::set(std::ptr::null_mut(), &mut win, std::ptr::null_mut());

        let mut rettv = TypvalT::default();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
            f_bufnr(&[num(999), num(1)], &mut rettv);
        }));
        assert!(result.is_err(), "expected a panic (buflist_new not yet translated)");
    }
}
