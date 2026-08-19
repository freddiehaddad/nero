//! Translated from `src/nvim/context.c` (tractable core only).
//!
//! `context.c` (~950 lines) implements the `:mkview`/context-API
//! snapshot machinery AND the temporary window/buffer-switch
//! machinery used by autocommand execution (`ctx_switch`/
//! `ctx_restore`, e.g. to run autocmds "as if" a different
//! window/buffer were current, then switch back). Window-target
//! switching/restoration is translated, including no-event and
//! no-display tab switches. Buffer targets, cwd preservation, and
//! display-changing tab switches remain deferred.
//!
//! This one check is enough to make `src/autocmd.rs`'s
//! `apply_autocmds` family real: every call site there constructs its
//! own `CtxSwitch::default()` (`cs_mode` defaults to
//! `CtxSwitchMode::None`, matching the original's `CtxSwitch aco =
//! { 0 }`) and NEVER calls a (not-yet-translated) `ctx_switch` on it
//! before calling [`ctx_restore`] - so the early-return branch is not
//! just reachable, it is the ONLY branch ever exercised anywhere in
//! this crate today, matching [`ctx_restore`]'s own doc comment
//! (translated near-verbatim below) which explicitly names exactly
//! this "skipped `ctx_switch()`" usage pattern as a first-class,
//! intentional no-op case - not an edge case being special-cased away.
//!
//! Also translated: [`ctx_saved_curwin`] (the window that was current
//! when the outermost `ctx_switch()` began) and
//! [`ctx_restore_curwin`] (restoring `curwin`/`curbuf`/`prevwin` from
//! a `CtxSwitch`), both reachable now that
//! `crate::window::win_find_by_handle` is real.
//!
//! `ctx_free` (frees a `Context`'s own `regs`/`jumps`/`bufs`/`gvars`/
//! `funcs` fields) needs NO Rust equivalent at all: `context_defs.rs`'s
//! `Context` already models every one of those fields as an owned
//! `Option<Vec<u8>>`/`Vec<Object>`, so Rust's own `Drop` impl already
//! performs the exact same cleanup automatically - the same reasoning
//! already established for `optval_free`/`ga_clear_strings` elsewhere
//! in this crate.

use crate::context_defs::{CtxSwitch, CtxSwitchMode, CtxWin};

/// The `ctx_win[]` pool of temporary "autocmd window" scratch windows
/// (`ctx_win_vec`, `context.h`'s `kvec_t(CtxWin)` - modeled as a plain
/// growable `Vec`, matching this crate's own established idiom for a
/// C `kvec_t`). Always empty today - nothing in this crate can
/// currently allocate a real autocmd window (`win_alloc`/the
/// window-splitting machinery needed by the not-yet-translated
/// `ctx_win_get`/`ctx_win_release` pair, `context.c`'s own
/// producer/consumer of this pool, neither translated).
pub(crate) static CTX_WIN_VEC: std::sync::LazyLock<crate::globals::GlobalCell<Vec<CtxWin>>> =
    std::sync::LazyLock::new(|| crate::globals::GlobalCell::new(Vec::new()));

/// Whether `win` is an active entry in `CTX_WIN_VEC` (the pool of
/// temporary scratch windows) (`is_ctx_win`).
///
/// # Safety
/// `win` need not be dereferenced (only ever compared by pointer
/// value against each pool entry's own `cw_win`) - safe to call with
/// any pointer, including a dangling or null one.
#[must_use]
pub fn is_ctx_win(win: *mut crate::buffer_defs::WinT) -> bool {
    // SAFETY: no overlapping live access - see this crate's
    // established GlobalCell::get_mut convention.
    unsafe { CTX_WIN_VEC.get_mut() }.iter().any(|cw| cw.cw_used && std::ptr::eq(cw.cw_win, win))
}

/// `_ctx_saved_curwin` - the window that was current when the
/// outermost `ctx_switch()` began.
///
/// Only ever set by `ctx_switch`/`ctx_restore` (neither translated),
/// so this stays `0` in this crate today, matching this file's own
/// established treatment of `CTX_WIN_VEC`.
static CTX_SAVED_CURWIN: crate::globals::GlobalCell<crate::types_defs::HandleT> =
    crate::globals::GlobalCell::new(0);

/// The window that was current when the outermost `ctx_switch()`
/// began, or null if no switch is in progress (`ctx_saved_curwin`).
///
/// A `0` handle is the "nothing saved" sentinel and is checked BEFORE
/// the lookup, so it never reaches `win_find_by_handle`.
///
/// # Safety
/// Forwarded from [`crate::window::win_find_by_handle`]'s own safety
/// doc.
#[must_use]
pub unsafe fn ctx_saved_curwin() -> *mut crate::buffer_defs::WinT {
    // SAFETY: a plain read through one exclusive borrow.
    let handle = unsafe { *CTX_SAVED_CURWIN.get_mut() };
    if handle == 0 {
        return std::ptr::null_mut();
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::window::win_find_by_handle(handle) }
}

/// Restore `curwin`/`curbuf` and `prevwin` from `cs`, entering
/// `fallback` if the saved window no longer exists
/// (`ctx_restore_curwin`).
///
/// Note the asymmetry the original has: `curwin` is only reassigned
/// when a window was actually found (so a vanished window with no
/// fallback leaves the current one alone), whereas `prevwin` is
/// assigned unconditionally and so may legitimately become null.
///
/// # Safety
/// Forwarded from [`crate::window::win_find_by_handle`]'s own safety
/// doc; the resolved window's `w_buffer` must be valid.
pub unsafe fn ctx_restore_curwin(cs: &CtxSwitch, fallback: *mut crate::buffer_defs::WinT) {
    // SAFETY: forwarded from this function's own safety doc.
    let mut save_curwin = unsafe { crate::window::win_find_by_handle(cs.cs_curwin) };
    if save_curwin.is_null() {
        save_curwin = fallback; // Hmm, original window disappeared.
    }
    if !save_curwin.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        globals.curwin = save_curwin;
        // SAFETY: forwarded from this function's own safety doc.
        globals.curbuf = unsafe { &*save_curwin }.w_buffer;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let prevwin = unsafe { crate::window::win_find_by_handle(cs.cs_prevwin) };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::globals::GLOBALS.get_mut() }.prevwin = prevwin;
}

/// Temporarily switch the current window (`ctx_switch`, window target).
///
/// # Safety
/// `wp`, the current globals, and their buffer/window lists must point
/// at live objects for the duration of the switch and matching restore.
pub unsafe fn ctx_switch(
    cs: &mut CtxSwitch,
    wp: *mut crate::buffer_defs::WinT,
    tp: *mut crate::buffer_defs::TabpageT,
    buf: *mut crate::buffer_defs::BufT,
    flags: i32,
) -> bool {
    debug_assert_ne!(wp.is_null(), buf.is_null());
    if !buf.is_null() {
        unimplemented!(
            "ctx_switch: buffer targets need temporary autocmd-window support"
        );
    }
    if flags & crate::context_defs::ctx_switch_flags::KEEP_CWD != 0 {
        unimplemented!("ctx_switch: KEEP_CWD needs ctx_cwd_save");
    }

    *cs = CtxSwitch::default();
    cs.cs_flags = flags;
    cs.cs_mode = CtxSwitchMode::Win;
    cs.cs_ctxwin_idx = -1;
    if !unsafe { crate::window::win_valid(wp) } {
        return false;
    }
    if flags & crate::context_defs::ctx_switch_flags::VALIDATE != 0 {
        cs.cs_target_win = unsafe { (*wp).handle };
        cs.cs_target_old_pos = unsafe { (*wp).w_cursor };
    }

    {
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        cs.cs_curwin = unsafe { (*globals.curwin).handle };
        cs.cs_prevwin = if globals.prevwin.is_null() {
            0
        } else {
            unsafe { (*globals.prevwin).handle }
        };
        cs.cs_same_win = std::ptr::eq(wp, globals.curwin);
        if unsafe { crate::buffer::bt_prompt(Some(&*globals.curbuf)) } {
            cs.cs_prompt_insert = unsafe { (*globals.curbuf).b_prompt_insert };
        }
        if !cs.cs_same_win {
            cs.cs_visual_active = globals.Visual.active;
            globals.Visual.active = false;
        }
    }

    if flags & crate::context_defs::ctx_switch_flags::NO_EVENTS != 0 {
        crate::autocmd::block_autocmds();
    }
    if !tp.is_null() {
        cs.cs_curtab = unsafe { crate::globals::GLOBALS.get_mut() }.curtab;
        if flags & crate::context_defs::ctx_switch_flags::NO_DISPLAY == 0 {
            unimplemented!(
                "ctx_switch: display-changing tab switches need goto_tabpage_tp"
            );
        }
        unsafe {
            crate::window::unuse_tabpage(
                crate::globals::GLOBALS.get_mut().curtab,
            );
            crate::window::use_tabpage(tp);
        }
    }
    let curbuf = {
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        globals.curwin = wp;
        globals.curbuf = unsafe { (*wp).w_buffer };
        globals.curbuf
    };
    cs.cs_new_curwin = unsafe { (*wp).handle };
    unsafe { crate::buffer::set_bufref(&mut cs.cs_new_curbuf, Some(&*curbuf)) };
    if flags & crate::context_defs::ctx_switch_flags::VALIDATE != 0 {
        unsafe { crate::cursor::check_cursor(wp) };
    }
    true
}

/// Undoes `ctx_switch()`: restores the previous location (if
/// possible) and the kept state.
///
/// No-op if `cs` was zero-initialized (`cs.cs_mode ==
/// `CtxSwitchMode::None`), even if `ctx_switch()` was not called on
/// it.
///
/// # Panics
/// Panics for buffer targets, cwd-preserving switches, or
/// display-changing tab switches, which still need their own
/// substantial subsystems.
///
/// # Safety
/// The saved/current window, tabpage, and buffer pointers must remain
/// live through the matching switch/restore pair.
pub unsafe fn ctx_restore(cs: &CtxSwitch) {
    if cs.cs_mode == CtxSwitchMode::None {
        return; // zero-initialized: ctx_switch() was never called on `cs`.
    }
    if cs.cs_mode != CtxSwitchMode::Win {
        unimplemented!(
            "ctx_restore: buffer targets need temporary autocmd-window support"
        );
    }
    if !cs.cs_curtab.is_null()
        && unsafe { crate::window::valid_tabpage(cs.cs_curtab) }
    {
        if cs.cs_flags & crate::context_defs::ctx_switch_flags::NO_DISPLAY == 0 {
            unimplemented!(
                "ctx_restore: display-changing tab switches need goto_tabpage_tp"
            );
        }
        let current_tab = unsafe { crate::globals::GLOBALS.get_mut() }.curtab;
        let old_current = unsafe { (*current_tab).tp_curwin };
        unsafe {
            crate::window::unuse_tabpage(current_tab);
            (*current_tab).tp_curwin = old_current;
            crate::window::use_tabpage(cs.cs_curtab);
        }
    }
    unsafe { ctx_restore_curwin(cs, std::ptr::null_mut()) };
    if !cs.cs_same_win {
        unsafe { crate::globals::GLOBALS.get_mut() }.Visual.active =
            cs.cs_visual_active;
    }
    if cs.cs_flags & crate::context_defs::ctx_switch_flags::NO_EVENTS != 0 {
        crate::autocmd::unblock_autocmds();
    }
    if cs.cs_flags & crate::context_defs::ctx_switch_flags::KEEP_CWD != 0 {
        unimplemented!("ctx_restore: KEEP_CWD needs ctx_cwd_restore");
    }
    if cs.cs_flags & crate::context_defs::ctx_switch_flags::VALIDATE != 0 {
        let target =
            unsafe { crate::window::win_find_by_handle(cs.cs_target_win) };
        if !target.is_null()
            && unsafe { (*target).w_cursor } != cs.cs_target_old_pos
        {
            unsafe { (*target).w_redr_status = true };
        }
        let current = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
        unsafe { crate::cursor::check_cursor(current) };
        if unsafe { crate::globals::GLOBALS.get_mut() }.Visual.active {
            let buf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
            let globals = crate::globals::GLOBALS.as_ptr();
            let visual_start =
                unsafe { std::ptr::addr_of_mut!((*globals).Visual.start) };
            unsafe { crate::cursor::check_pos(&mut *buf, &mut *visual_start) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- ctx_saved_curwin / ctx_restore_curwin ---

    /// Saves and restores `curwin`/`curbuf`/`prevwin` across a test,
    /// even through a panic, so a failing test cannot leave dangling
    /// pointers in the globals for whichever test runs next.
    struct CurwinGuard {
        curwin: *mut crate::buffer_defs::WinT,
        curbuf: *mut crate::buffer_defs::BufT,
        prevwin: *mut crate::buffer_defs::WinT,
        firstwin: *mut crate::buffer_defs::WinT,
        visual_active: bool,
    }

    impl CurwinGuard {
        fn save() -> Self {
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            Self {
                curwin: g.curwin,
                curbuf: g.curbuf,
                prevwin: g.prevwin,
                firstwin: g.firstwin,
                visual_active: g.Visual.active,
            }
        }
    }

    impl Drop for CurwinGuard {
        fn drop(&mut self) {
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            g.curwin = self.curwin;
            g.curbuf = self.curbuf;
            g.prevwin = self.prevwin;
            g.firstwin = self.firstwin;
            g.Visual.active = self.visual_active;
        }
    }

    /// A zero handle means "nothing saved" and must be answered
    /// without ever consulting the window list.
    #[test]
    fn ctx_saved_curwin_is_null_when_nothing_was_saved() {
        let _lock = crate::globals::global_state_test_lock();
        assert!(unsafe { ctx_saved_curwin() }.is_null());
    }

    #[test]
    fn ctx_restore_curwin_restores_the_saved_window_and_its_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = CurwinGuard::save();

        let mut buf = Box::new(crate::buffer_defs::BufT::default());
        let mut win = Box::new(crate::buffer_defs::WinT::default());
        win.handle = 42;
        win.w_buffer = std::ptr::from_mut(&mut *buf);
        win.w_next = std::ptr::null_mut();

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        globals.firstwin = std::ptr::from_mut(&mut *win);
        globals.curwin = std::ptr::null_mut();
        globals.curbuf = std::ptr::null_mut();

        let cs = CtxSwitch { cs_curwin: 42, cs_prevwin: 0, ..Default::default() };
        unsafe { ctx_restore_curwin(&cs, std::ptr::null_mut()) };

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        assert!(std::ptr::eq(globals.curwin, std::ptr::from_mut(&mut *win)));
        assert!(std::ptr::eq(globals.curbuf, std::ptr::from_mut(&mut *buf)));
        // A zero prevwin handle resolves to nothing.
        assert!(globals.prevwin.is_null());
    }

    /// When the saved window is gone, the fallback is entered instead.
    #[test]
    fn ctx_restore_curwin_enters_the_fallback_when_the_saved_window_vanished() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = CurwinGuard::save();

        let mut buf = Box::new(crate::buffer_defs::BufT::default());
        let mut fallback = Box::new(crate::buffer_defs::WinT::default());
        fallback.handle = 7;
        fallback.w_buffer = std::ptr::from_mut(&mut *buf);

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        globals.firstwin = std::ptr::null_mut(); // no windows to find
        globals.curwin = std::ptr::null_mut();

        // Handle 999 does not exist.
        let cs = CtxSwitch { cs_curwin: 999, cs_prevwin: 0, ..Default::default() };
        unsafe { ctx_restore_curwin(&cs, std::ptr::from_mut(&mut *fallback)) };

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        assert!(std::ptr::eq(globals.curwin, std::ptr::from_mut(&mut *fallback)));
        assert!(std::ptr::eq(globals.curbuf, std::ptr::from_mut(&mut *buf)));
    }

    /// The asymmetry: with the saved window gone AND no fallback,
    /// `curwin` is left alone rather than being nulled - but
    /// `prevwin` is assigned unconditionally and so does become null.
    #[test]
    fn ctx_restore_curwin_leaves_curwin_alone_but_still_clears_prevwin() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = CurwinGuard::save();

        let mut existing = Box::new(crate::buffer_defs::WinT::default());
        existing.handle = 3;
        let mut stale_prev = Box::new(crate::buffer_defs::WinT::default());
        stale_prev.handle = 4;

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        globals.firstwin = std::ptr::null_mut(); // nothing findable
        globals.curwin = std::ptr::from_mut(&mut *existing);
        globals.prevwin = std::ptr::from_mut(&mut *stale_prev);

        let cs = CtxSwitch { cs_curwin: 999, cs_prevwin: 998, ..Default::default() };
        unsafe { ctx_restore_curwin(&cs, std::ptr::null_mut()) };

        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        assert!(
            std::ptr::eq(globals.curwin, std::ptr::from_mut(&mut *existing)),
            "curwin must survive a vanished window with no fallback"
        );
        assert!(
            globals.prevwin.is_null(),
            "prevwin is assigned unconditionally, so it may become null"
        );
    }

    #[test]
    fn ctx_switch_and_restore_round_trip_a_window_target() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = CurwinGuard::save();
        let mut first_buf = crate::buffer_defs::BufT::default();
        let first_buf_ptr = std::ptr::addr_of_mut!(first_buf);
        let mut second_buf = crate::buffer_defs::BufT::default();
        let second_buf_ptr = std::ptr::addr_of_mut!(second_buf);
        let mut second = crate::buffer_defs::WinT {
            handle: 2,
            w_buffer: second_buf_ptr,
            ..Default::default()
        };
        let second_ptr = std::ptr::addr_of_mut!(second);
        let mut first = crate::buffer_defs::WinT {
            handle: 1,
            w_buffer: first_buf_ptr,
            w_next: second_ptr,
            ..Default::default()
        };
        let first_ptr = std::ptr::addr_of_mut!(first);
        {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            globals.firstwin = first_ptr;
            globals.curwin = first_ptr;
            globals.curbuf = first_buf_ptr;
            globals.prevwin = std::ptr::null_mut();
            globals.Visual.active = true;
        }
        let mut switch = CtxSwitch::default();

        assert!(unsafe {
            ctx_switch(
                &mut switch,
                second_ptr,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                crate::context_defs::ctx_switch_flags::NO_EVENTS,
            )
        });

        assert_eq!(switch.cs_mode, CtxSwitchMode::Win);
        assert_eq!(switch.cs_curwin, 1);
        assert_eq!(switch.cs_new_curwin, 2);
        assert!(!switch.cs_same_win);
        assert!(switch.cs_visual_active);
        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.curwin, second_ptr);
        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.curbuf, second_buf_ptr);
        assert!(!unsafe { crate::globals::GLOBALS.get_mut() }.Visual.active);
        assert!(crate::autocmd::is_autocmd_blocked());

        unsafe { ctx_restore(&switch) };

        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.curwin, first_ptr);
        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.curbuf, first_buf_ptr);
        assert!(unsafe { crate::globals::GLOBALS.get_mut() }.Visual.active);
        assert!(!crate::autocmd::is_autocmd_blocked());
    }

    #[test]
    fn ctx_restore_is_a_noop_for_a_default_zeroed_ctx_switch() {
        let cs = CtxSwitch::default();
        unsafe { ctx_restore(&cs) }; // must not panic
    }

    #[test]
    #[should_panic(expected = "buffer targets")]
    fn ctx_restore_panics_for_a_buffer_mode() {
        let cs = CtxSwitch { cs_mode: CtxSwitchMode::Buf, ..Default::default() };
        unsafe { ctx_restore(&cs) };
    }

    #[test]
    fn is_ctx_win_false_when_pool_is_empty() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = crate::buffer_defs::WinT::default();
        assert!(unsafe { CTX_WIN_VEC.get_mut() }.is_empty());
        assert!(!is_ctx_win(&mut win as *mut crate::buffer_defs::WinT));
    }

    #[test]
    fn is_ctx_win_true_for_a_used_entry_matching_the_window() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = crate::buffer_defs::WinT::default();
        let win_ptr = &mut win as *mut crate::buffer_defs::WinT;
        unsafe { CTX_WIN_VEC.get_mut() }.push(CtxWin { cw_win: win_ptr, cw_used: true });

        assert!(is_ctx_win(win_ptr));

        unsafe { CTX_WIN_VEC.get_mut() }.clear();
    }

    #[test]
    fn is_ctx_win_false_for_an_unused_entry() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = crate::buffer_defs::WinT::default();
        let win_ptr = &mut win as *mut crate::buffer_defs::WinT;
        unsafe { CTX_WIN_VEC.get_mut() }.push(CtxWin { cw_win: win_ptr, cw_used: false });

        assert!(!is_ctx_win(win_ptr));

        unsafe { CTX_WIN_VEC.get_mut() }.clear();
    }

    #[test]
    fn is_ctx_win_false_for_a_different_window() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win_a = crate::buffer_defs::WinT::default();
        let mut win_b = crate::buffer_defs::WinT::default();
        let ptr_a = &mut win_a as *mut crate::buffer_defs::WinT;
        let ptr_b = &mut win_b as *mut crate::buffer_defs::WinT;
        unsafe { CTX_WIN_VEC.get_mut() }.push(CtxWin { cw_win: ptr_a, cw_used: true });

        assert!(!is_ctx_win(ptr_b));

        unsafe { CTX_WIN_VEC.get_mut() }.clear();
    }
}
