//! Translated from `src/nvim/context.c` (tractable core only).
//!
//! `context.c` (~950 lines) implements the `:mkview`/context-API
//! snapshot machinery AND the temporary window/buffer-switch
//! machinery used by autocommand execution (`ctx_switch`/
//! `ctx_restore`, e.g. to run autocmds "as if" a different
//! window/buffer were current, then switch back). Window-target
//! switching/restoration is translated, including no-event/no-display
//! tab switches. Buffer targets use an existing window when available
//! or a pooled temporary autocmd window otherwise. Cwd and
//! `'autochdir'` preservation are real too. Display-changing tab
//! switches, recovery when a temporary window was moved to another
//! tab, and `:lcd` directory repair in a temporary window remain
//! deferred.
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
/// C `kvec_t`).
pub(crate) static CTX_WIN_VEC: std::sync::LazyLock<crate::globals::GlobalCell<Vec<CtxWin>>> =
    std::sync::LazyLock::new(|| crate::globals::GlobalCell::new(Vec::new()));
static NEXT_CTX_WIN_HANDLE: crate::globals::GlobalCell<crate::types_defs::HandleT> =
    crate::globals::GlobalCell::new(1_000_000_000);

/// Convert a readfile-style API Array into serialized bytes
/// (`array_to_string`).
///
/// # Safety
/// Allocates temporary eval containers and mutates their GC registry.
#[must_use]
pub unsafe fn array_to_string(
    array: &[crate::api::private::defs::Object],
    error: &mut crate::api::private::defs::Error,
) -> Vec<u8> {
    let value = unsafe {
        crate::api::private::converter::object_to_vim(
            &crate::api::private::defs::Object::Array(
                array.to_vec(),
            ),
            error,
        )
    };
    let crate::eval::typval_defs::TypvalValue::List(list) =
        value.value
    else {
        unreachable!("Object::Array always converts to a List")
    };
    let output =
        unsafe { crate::eval::encode::encode_vim_list_to_buf(list) };
    if output.is_none() {
        error.r#type =
            crate::api::private::defs::ErrorType::Exception;
        error.msg = Some(
            "E474: Failed to convert list to msgpack string buffer"
                .to_owned(),
        );
    }
    unsafe {
        crate::eval::typval::tv_clear_simple(
            &crate::eval::typval_defs::TypvalT {
                value: crate::eval::typval_defs::TypvalValue::List(
                    list,
                ),
                ..Default::default()
            },
        );
    }
    output.unwrap_or_default()
}

#[cfg(test)]
unsafe fn reset_ctx_win_pool_for_test() {
    let entries = std::mem::take(unsafe { CTX_WIN_VEC.get_mut() });
    for entry in entries {
        if !entry.cw_win.is_null() {
            let vars = unsafe { (*entry.cw_win).w_vars };
            if !vars.is_null() {
                unsafe { crate::eval::typval::tv_dict_free(vars) };
            }
            unsafe { drop(Box::from_raw(entry.cw_win)) };
        }
    }
    unsafe { *NEXT_CTX_WIN_HANDLE.get_mut() = 1_000_000_000 };
}

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

unsafe fn ctx_win_alloc(index: usize) -> *mut crate::buffer_defs::WinT {
    let handle = unsafe { NEXT_CTX_WIN_HANDLE.get_mut() };
    let mut win = Box::new(crate::buffer_defs::WinT::default());
    win.handle = *handle;
    *handle += 1;
    win.w_config.width = unsafe { crate::globals::GLOBALS.get_mut() }.Columns;
    win.w_config.height = 5;
    win.w_config.focusable = false;
    win.w_config.mouse = false;
    win.w_config.hide = true;
    win.w_vars = crate::eval::typval::tv_dict_alloc();
    unsafe {
        crate::eval::vars::init_var_dict(
            &mut *win.w_vars,
            &mut win.w_winvar,
            crate::eval::typval_defs::ScopeType::Scope,
        )
    };
    let win = Box::into_raw(win);
    let entries = unsafe { CTX_WIN_VEC.get_mut() };
    entries[index].cw_win = win;
    win
}

unsafe fn ctx_win_prep(
    cs: &mut CtxSwitch,
    buf: *mut crate::buffer_defs::BufT,
) -> *mut crate::buffer_defs::WinT {
    let index = {
        let windows = unsafe { CTX_WIN_VEC.get_mut() };
        windows
            .iter()
            .position(|entry| !entry.cw_used)
            .unwrap_or_else(|| {
                windows.push(CtxWin::default());
                windows.len() - 1
            })
    };
    let existing = unsafe { CTX_WIN_VEC.get_mut() }[index].cw_win;
    let win = if existing.is_null() {
        unsafe { ctx_win_alloc(index) }
    } else {
        existing
    };
    let entries = unsafe { CTX_WIN_VEC.get_mut() };
    entries[index].cw_used = true;
    cs.cs_ctxwin_idx = index as i32;

    unsafe {
        (*win).w_buffer = buf;
        (*win).w_s = std::ptr::addr_of_mut!((*buf).b_s);
        (*buf).b_nwindows += 1;
        (*win).w_lines_valid = 0;
        (*win).w_cursor = crate::pos_defs::PosT {
            lnum: 1,
            col: 0,
            coladd: 0,
        };
        (*win).w_curswant = 0;
        (*win).w_pcmark.lnum = 1;
        (*win).w_pcmark.col = 0;
        (*win).w_prev_pcmark.lnum = 0;
        (*win).w_prev_pcmark.col = 0;
        (*win).w_topline = 1;
        (*win).w_topfill = 0;
        (*win).w_botline = 2;
        (*win).w_valid = 0;
        (*win).w_localdir = None;
        crate::drawscreen::redraw_later(
            win,
            crate::drawscreen::UPD_NOT_VALID,
        );
    }

    let current_tab = unsafe { crate::globals::GLOBALS.get_mut() }.curtab;
    cs.cs_tp_localdir = unsafe { (*current_tab).tp_localdir.take() };
    cs.cs_globaldir =
        unsafe { crate::globals::GLOBALS.get_mut() }.globaldir.take();
    let last = unsafe { crate::globals::GLOBALS.get_mut() }.lastwin;
    unsafe { crate::window::win_append(last, win, std::ptr::null_mut()) };
    win
}

unsafe fn ctx_win_rest(cs: &CtxSwitch) -> *mut crate::buffer_defs::WinT {
    let index = cs.cs_ctxwin_idx as usize;
    let win = unsafe { CTX_WIN_VEC.get_mut() }[index].cw_win;
    if !std::ptr::eq(
        unsafe { crate::globals::GLOBALS.get_mut() }.curwin,
        win,
    ) {
        unimplemented!(
            "ctx_win_rest: finding a moved autocmd window needs goto_tabpage_tp/win_goto"
        );
    }
    unsafe {
        (*(*win).w_buffer).b_nwindows -= 1;
        crate::window::win_remove(win, std::ptr::null_mut());
        CTX_WIN_VEC.get_mut()[index].cw_used = false;
    }
    win
}

/// `_ctx_saved_curwin` - the window that was current when the
/// outermost `ctx_switch()` began.
///
/// Set and cleared by the real [`ctx_switch`]/[`ctx_restore`] buffer
/// target lifecycle.
static CTX_SAVED_CURWIN: crate::globals::GlobalCell<crate::types_defs::HandleT> =
    crate::globals::GlobalCell::new(0);
static CTX_SWITCH_DEPTH: crate::globals::GlobalCell<i32> =
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

unsafe fn ctx_cwd_save(
    cs: &mut CtxSwitch,
    wp: *mut crate::buffer_defs::WinT,
    tp: *mut crate::buffer_defs::TabpageT,
) {
    cs.cs_cwd_status = crate::vim_defs::FAIL;
    let autochdir = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_acd != 0;
    let globals = unsafe { crate::globals::GLOBALS.get_mut() };
    let target_tab = if tp.is_null() { globals.curtab } else { tp };
    let need_save = !std::ptr::eq(globals.curwin, wp)
        && (unsafe { (*globals.curwin).w_localdir.is_some() }
            || (!wp.is_null() && unsafe { (*wp).w_localdir.is_some() })
            || (!std::ptr::eq(globals.curtab, target_tab)
                && (unsafe { (*globals.curtab).tp_localdir.is_some() }
                    || unsafe { (*target_tab).tp_localdir.is_some() }))
            || autochdir);
    if need_save
        && let Some(cwd) = crate::os::fs::os_dirname()
    {
        cs.cs_cwd = Some(cwd);
        cs.cs_cwd_status = crate::vim_defs::OK;
    }
    if cs.cs_cwd_status == crate::vim_defs::OK && autochdir {
        let curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
        let (sfname, fname) = unsafe {
            ((*curbuf).b_sfname.clone(), (*curbuf).b_fname.clone())
        };
        if sfname.is_some() && sfname == fname {
            cs.cs_save_sfname = sfname;
        }
        unsafe { crate::buffer::do_autochdir() };
        cs.cs_apply_acd = crate::os::fs::os_dirname().as_deref()
            == cs.cs_cwd.as_deref();
    }
}

fn bytes_path(path: &[u8]) -> Option<std::path::PathBuf> {
    std::str::from_utf8(path)
        .ok()
        .map(std::path::PathBuf::from)
}

unsafe fn ctx_cwd_restore(cs: &CtxSwitch) {
    if cs.cs_apply_acd {
        unsafe { crate::buffer::do_autochdir() };
    } else if cs.cs_cwd_status == crate::vim_defs::OK
        && let Some(path) = cs.cs_cwd.as_deref().and_then(bytes_path)
    {
        let _ = crate::os::fs::os_chdir(&path);
        if let Some(saved) = &cs.cs_save_sfname {
            let curbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
            unsafe {
                (*curbuf).b_sfname = Some(saved.clone());
                (*curbuf).b_fname = Some(saved.clone());
            }
        }
    }
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
    debug_assert!(buf.is_null() || tp.is_null());
    let mut target = wp;
    let mode = if buf.is_null() {
        CtxSwitchMode::Win
    } else {
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        if std::ptr::eq(buf, globals.curbuf) {
            target = globals.curwin;
        } else {
            let mut candidate = globals.firstwin;
            while !candidate.is_null() {
                if std::ptr::eq(unsafe { (*candidate).w_buffer }, buf) {
                    target = candidate;
                    break;
                }
                candidate = unsafe { (*candidate).w_next };
            }
        }
        CtxSwitchMode::Buf
    };
    *cs = CtxSwitch::default();
    cs.cs_flags = flags;
    cs.cs_mode = mode;
    cs.cs_ctxwin_idx = -1;
    if flags & crate::context_defs::ctx_switch_flags::VALIDATE != 0
        && !target.is_null()
    {
        cs.cs_target_win = unsafe { (*target).handle };
        cs.cs_target_old_pos = unsafe { (*target).w_cursor };
    }
    if flags & crate::context_defs::ctx_switch_flags::KEEP_CWD != 0 {
        unsafe {
            ctx_cwd_save(
                cs,
                target,
                if tp.is_null() {
                    crate::globals::GLOBALS.get_mut().curtab
                } else {
                    tp
                },
            )
        };
    }

    {
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        cs.cs_curwin = unsafe { (*globals.curwin).handle };
        cs.cs_prevwin = if globals.prevwin.is_null() {
            0
        } else {
            unsafe { (*globals.prevwin).handle }
        };
        cs.cs_same_win = std::ptr::eq(target, globals.curwin);
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
    if mode == CtxSwitchMode::Buf && target.is_null() {
        target = unsafe { ctx_win_prep(cs, buf) };
        let current = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
        unsafe { crate::window::leaving_window(current) };
        unsafe { crate::globals::GLOBALS.get_mut() }.prevwin = current;
    }
    if mode == CtxSwitchMode::Buf {
        debug_assert!(unsafe { crate::window::win_valid(target) });
    } else if !unsafe { crate::window::win_valid(target) } {
        return false;
    }
    let curbuf = {
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        globals.curwin = target;
        globals.curbuf = unsafe { (*target).w_buffer };
        globals.curbuf
    };
    cs.cs_new_curwin = unsafe { (*target).handle };
    unsafe { crate::buffer::set_bufref(&mut cs.cs_new_curbuf, Some(&*curbuf)) };
    if mode == CtxSwitchMode::Buf && cs.cs_new_curwin != cs.cs_curwin {
        let depth = unsafe { CTX_SWITCH_DEPTH.get_mut() };
        if *depth == 0 {
            unsafe { *CTX_SAVED_CURWIN.get_mut() = cs.cs_curwin };
        }
        *depth += 1;
    }
    if flags & crate::context_defs::ctx_switch_flags::VALIDATE != 0 {
        unsafe { crate::cursor::check_cursor(target) };
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
/// Panics for temporary autocmd-window targets, cwd-preserving
/// switches, or display-changing tab switches, which still need their
/// own substantial subsystems.
///
/// # Safety
/// The saved/current window, tabpage, and buffer pointers must remain
/// live through the matching switch/restore pair.
pub unsafe fn ctx_restore(cs: &CtxSwitch) {
    if cs.cs_mode == CtxSwitchMode::None {
        return; // zero-initialized: ctx_switch() was never called on `cs`.
    }
    if cs.cs_mode == CtxSwitchMode::Win
        && !cs.cs_curtab.is_null()
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
    let mut temp_win = std::ptr::null_mut();
    if cs.cs_mode == CtxSwitchMode::Buf {
        if cs.cs_ctxwin_idx >= 0 {
            temp_win = unsafe { ctx_win_rest(cs) };
        } else {
            let current_win = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
            let current_buf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
            let saved_buf = cs.cs_new_curbuf.br_buf;
            if unsafe { (*current_win).handle } == cs.cs_new_curwin
                && !std::ptr::eq(current_buf, saved_buf)
                && unsafe { crate::buffer::bufref_valid(&cs.cs_new_curbuf) }
                && !unsafe { (*saved_buf).b_ml.ml_mfp }.is_null()
            {
                let old_s = unsafe { std::ptr::addr_of_mut!((*current_buf).b_s) };
                if std::ptr::eq(unsafe { (*current_win).w_s }, old_s) {
                    unsafe {
                        (*current_win).w_s =
                            std::ptr::addr_of_mut!((*saved_buf).b_s);
                    }
                }
                unsafe {
                    (*current_buf).b_nwindows -= 1;
                    crate::globals::GLOBALS.get_mut().curbuf = saved_buf;
                    (*current_win).w_buffer = saved_buf;
                    (*saved_buf).b_nwindows += 1;
                }
            }
        }
    }
    let fallback = if temp_win.is_null() {
        std::ptr::null_mut()
    } else {
        unsafe { crate::globals::GLOBALS.get_mut() }.firstwin
    };
    unsafe { ctx_restore_curwin(cs, fallback) };
    if !temp_win.is_null() {
        let globals = crate::globals::GLOBALS.as_ptr();
        let current = unsafe { (*globals).curwin };
        unsafe { crate::window::entering_window(current) };
        let curbuf = unsafe { (*globals).curbuf };
        if unsafe { crate::buffer::bt_prompt(Some(&*curbuf)) } {
            unsafe { (*curbuf).b_prompt_insert = cs.cs_prompt_insert };
        }
        let vars = unsafe { (*temp_win).w_vars };
        if !vars.is_null() {
            unsafe { crate::eval::vars::vars_clear(&mut *vars) };
        }
        if unsafe { (*temp_win).w_localdir.is_some() } {
            unimplemented!(
                "ctx_restore: :lcd in an autocmd window needs win_fix_current_dir"
            );
        }
        let curtab = unsafe { (*globals).curtab };
        unsafe {
            (*curtab).tp_localdir = cs.cs_tp_localdir.clone();
            (*globals).globaldir = cs.cs_globaldir.clone();
        }
        if unsafe { (*current).w_topline > (*curbuf).b_ml.ml_line_count } {
            unsafe {
                (*current).w_topline = (*curbuf).b_ml.ml_line_count;
                (*current).w_topfill = 0;
            }
        }
    }
    if !cs.cs_same_win {
        unsafe { crate::globals::GLOBALS.get_mut() }.Visual.active =
            cs.cs_visual_active;
    }
    if cs.cs_mode == CtxSwitchMode::Buf {
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
    if cs.cs_flags & crate::context_defs::ctx_switch_flags::NO_EVENTS != 0 {
        crate::autocmd::unblock_autocmds();
    }
    if cs.cs_flags & crate::context_defs::ctx_switch_flags::KEEP_CWD != 0 {
        unsafe { ctx_cwd_restore(cs) };
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
    if cs.cs_mode == CtxSwitchMode::Buf
        && cs.cs_new_curwin != cs.cs_curwin
    {
        let depth = unsafe { CTX_SWITCH_DEPTH.get_mut() };
        debug_assert!(*depth > 0);
        *depth -= 1;
        if *depth == 0 {
            unsafe { *CTX_SAVED_CURWIN.get_mut() = 0 };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn array_to_string_joins_readfile_style_lines() {
        let _lock = crate::globals::global_state_test_lock();
        let mut error = crate::api::private::defs::Error::default();
        let array = [
            crate::api::private::defs::Object::String(
                b"one".to_vec(),
            ),
            crate::api::private::defs::Object::String(Vec::new()),
            crate::api::private::defs::Object::String(
                b"two".to_vec(),
            ),
        ];
        assert_eq!(
            unsafe { array_to_string(&array, &mut error) },
            b"one\n\ntwo"
        );
        assert!(!error.is_set());
    }

    #[test]
    fn array_to_string_sets_e474_for_nonstring_items() {
        let _lock = crate::globals::global_state_test_lock();
        let mut error = crate::api::private::defs::Error::default();
        assert_eq!(
            unsafe {
                array_to_string(
                    &[crate::api::private::defs::Object::Integer(1)],
                    &mut error,
                )
            },
            Vec::<u8>::new()
        );
        assert_eq!(
            error.r#type,
            crate::api::private::defs::ErrorType::Exception
        );
        assert_eq!(
            error.msg.as_deref(),
            Some("E474: Failed to convert list to msgpack string buffer")
        );
    }

    // --- ctx_saved_curwin / ctx_restore_curwin ---

    /// Saves and restores `curwin`/`curbuf`/`prevwin` across a test,
    /// even through a panic, so a failing test cannot leave dangling
    /// pointers in the globals for whichever test runs next.
    struct CurwinGuard {
        curwin: *mut crate::buffer_defs::WinT,
        curbuf: *mut crate::buffer_defs::BufT,
        prevwin: *mut crate::buffer_defs::WinT,
        firstwin: *mut crate::buffer_defs::WinT,
        lastwin: *mut crate::buffer_defs::WinT,
        curtab: *mut crate::buffer_defs::TabpageT,
        first_tabpage: *mut crate::buffer_defs::TabpageT,
        topframe: *mut crate::buffer_defs::FrameT,
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
                lastwin: g.lastwin,
                curtab: g.curtab,
                first_tabpage: g.first_tabpage,
                topframe: g.topframe,
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
            g.lastwin = self.lastwin;
            g.curtab = self.curtab;
            g.first_tabpage = self.first_tabpage;
            g.topframe = self.topframe;
            g.Visual.active = self.visual_active;
        }
    }

    struct TempCwdGuard {
        original: std::path::PathBuf,
        root: std::path::PathBuf,
    }

    struct AutochdirGuard(i32);

    impl AutochdirGuard {
        fn set(value: i32) -> Self {
            let options = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            let previous = options.p_acd;
            options.p_acd = value;
            Self(previous)
        }
    }

    impl Drop for AutochdirGuard {
        fn drop(&mut self) {
            unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_acd = self.0;
        }
    }

    struct CtxWinPoolGuard;

    struct GlobaldirGuard(Option<Vec<u8>>);

    impl GlobaldirGuard {
        fn set(value: Option<Vec<u8>>) -> Self {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            let previous = std::mem::replace(&mut globals.globaldir, value);
            Self(previous)
        }
    }

    impl Drop for GlobaldirGuard {
        fn drop(&mut self) {
            unsafe { crate::globals::GLOBALS.get_mut() }.globaldir =
                self.0.take();
        }
    }

    impl CtxWinPoolGuard {
        fn reset() -> Self {
            unsafe { reset_ctx_win_pool_for_test() };
            Self
        }
    }

    impl Drop for CtxWinPoolGuard {
        fn drop(&mut self) {
            unsafe { reset_ctx_win_pool_for_test() };
        }
    }

    impl TempCwdGuard {
        fn new() -> Self {
            let original = std::env::current_dir().unwrap();
            let root = std::env::temp_dir().join(format!(
                "nero-context-cwd-{}-{}",
                std::process::id(),
                crate::profile::profile_start()
            ));
            std::fs::create_dir_all(root.join("one")).unwrap();
            std::fs::create_dir_all(root.join("two")).unwrap();
            std::env::set_current_dir(root.join("one")).unwrap();
            Self { original, root }
        }
    }

    impl Drop for TempCwdGuard {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.original);
            let _ = std::fs::remove_dir_all(&self.root);
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
    fn ctx_switch_validates_a_window_after_installing_its_tabpage() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = CurwinGuard::save();

        let mut current_buf = crate::buffer_defs::BufT::default();
        let current_buf_ptr = std::ptr::addr_of_mut!(current_buf);
        let mut target_buf = crate::buffer_defs::BufT::default();
        let target_buf_ptr = std::ptr::addr_of_mut!(target_buf);
        let mut current_win = crate::buffer_defs::WinT {
            handle: 31,
            w_buffer: current_buf_ptr,
            ..Default::default()
        };
        let current_win_ptr = std::ptr::addr_of_mut!(current_win);
        let mut target_win = crate::buffer_defs::WinT {
            handle: 32,
            w_buffer: target_buf_ptr,
            ..Default::default()
        };
        let target_win_ptr = std::ptr::addr_of_mut!(target_win);
        let command_height =
            unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ch;
        let mut target_tab = crate::buffer_defs::TabpageT {
            tp_curwin: target_win_ptr,
            tp_firstwin: target_win_ptr,
            tp_lastwin: target_win_ptr,
            tp_ch_used: command_height,
            ..Default::default()
        };
        let target_tab_ptr = std::ptr::addr_of_mut!(target_tab);
        let mut current_tab = crate::buffer_defs::TabpageT {
            tp_next: target_tab_ptr,
            tp_curwin: current_win_ptr,
            tp_firstwin: current_win_ptr,
            tp_lastwin: current_win_ptr,
            tp_ch_used: command_height,
            ..Default::default()
        };
        let current_tab_ptr = std::ptr::addr_of_mut!(current_tab);
        {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            globals.curbuf = current_buf_ptr;
            globals.curwin = current_win_ptr;
            globals.firstwin = current_win_ptr;
            globals.lastwin = current_win_ptr;
            globals.curtab = current_tab_ptr;
            globals.first_tabpage = current_tab_ptr;
            globals.prevwin = std::ptr::null_mut();
        }

        let mut switch = CtxSwitch::default();
        assert!(unsafe {
            ctx_switch(
                &mut switch,
                target_win_ptr,
                target_tab_ptr,
                std::ptr::null_mut(),
                crate::context_defs::ctx_switch_flags::NO_EVENTS
                    | crate::context_defs::ctx_switch_flags::NO_DISPLAY,
            )
        });
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        assert_eq!(globals.curtab, target_tab_ptr);
        assert_eq!(globals.curwin, target_win_ptr);
        assert_eq!(globals.curbuf, target_buf_ptr);
        assert!(crate::autocmd::is_autocmd_blocked());

        unsafe { ctx_restore(&switch) };
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        assert_eq!(globals.curtab, current_tab_ptr);
        assert_eq!(globals.curwin, current_win_ptr);
        assert_eq!(globals.curbuf, current_buf_ptr);
        assert!(!crate::autocmd::is_autocmd_blocked());
    }

    #[test]
    fn ctx_switch_uses_an_existing_window_for_a_buffer_target() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = CurwinGuard::save();
        let mut first_buf = crate::buffer_defs::BufT::default();
        let first_buf_ptr = std::ptr::addr_of_mut!(first_buf);
        let mut second_buf = crate::buffer_defs::BufT::default();
        let second_buf_ptr = std::ptr::addr_of_mut!(second_buf);
        let mut second = crate::buffer_defs::WinT {
            handle: 12,
            w_buffer: second_buf_ptr,
            ..Default::default()
        };
        let second_ptr = std::ptr::addr_of_mut!(second);
        let mut first = crate::buffer_defs::WinT {
            handle: 11,
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
        }
        let mut switch = CtxSwitch::default();

        assert!(unsafe {
            ctx_switch(
                &mut switch,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                second_buf_ptr,
                crate::context_defs::ctx_switch_flags::NO_EVENTS,
            )
        });

        assert_eq!(switch.cs_mode, CtxSwitchMode::Buf);
        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.curwin, second_ptr);
        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.curbuf, second_buf_ptr);
        assert_eq!(unsafe { ctx_saved_curwin() }, first_ptr);

        unsafe { ctx_restore(&switch) };

        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.curwin, first_ptr);
        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.curbuf, first_buf_ptr);
        assert!(unsafe { ctx_saved_curwin() }.is_null());
        assert!(!crate::autocmd::is_autocmd_blocked());
    }

    #[test]
    fn ctx_switch_keep_cwd_is_inert_without_local_directories() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = CurwinGuard::save();
        let mut buf = crate::buffer_defs::BufT::default();
        let buf_ptr = std::ptr::addr_of_mut!(buf);
        let mut win = crate::buffer_defs::WinT {
            handle: 21,
            w_buffer: buf_ptr,
            ..Default::default()
        };
        let win_ptr = std::ptr::addr_of_mut!(win);
        {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            globals.firstwin = win_ptr;
            globals.curwin = win_ptr;
            globals.curbuf = buf_ptr;
        }
        let mut switch = CtxSwitch::default();
        assert!(unsafe {
            ctx_switch(
                &mut switch,
                win_ptr,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                crate::context_defs::ctx_switch_flags::KEEP_CWD,
            )
        });
        assert_eq!(switch.cs_cwd_status, crate::vim_defs::FAIL);
        assert!(switch.cs_cwd.is_none());
        unsafe { ctx_restore(&switch) };
    }

    #[test]
    #[cfg_attr(
        miri,
        ignore = "Miri cannot emulate this test's real directory creation and chdir calls"
    )]
    fn ctx_switch_keep_cwd_restores_a_changed_directory() {
        let _lock = crate::globals::global_state_test_lock();
        let _cwd_lock = crate::os::fs::cwd_test_lock();
        let cwd = TempCwdGuard::new();
        let first_dir = std::env::current_dir().unwrap();
        let second_dir = cwd.root.join("two");
        let _g = CurwinGuard::save();
        let _acd = AutochdirGuard::set(0);
        let mut first_buf = crate::buffer_defs::BufT::default();
        let first_buf_ptr = std::ptr::addr_of_mut!(first_buf);
        let mut second_buf = crate::buffer_defs::BufT::default();
        let second_buf_ptr = std::ptr::addr_of_mut!(second_buf);
        let mut second = crate::buffer_defs::WinT {
            handle: 32,
            w_buffer: second_buf_ptr,
            w_localdir: Some(b"local".to_vec()),
            ..Default::default()
        };
        let second_ptr = std::ptr::addr_of_mut!(second);
        let mut first = crate::buffer_defs::WinT {
            handle: 31,
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
        }
        let mut switch = CtxSwitch::default();
        assert!(unsafe {
            ctx_switch(
                &mut switch,
                second_ptr,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                crate::context_defs::ctx_switch_flags::KEEP_CWD,
            )
        });
        assert_eq!(switch.cs_cwd_status, crate::vim_defs::OK);
        std::env::set_current_dir(&second_dir).unwrap();

        unsafe { ctx_restore(&switch) };

        assert_eq!(std::env::current_dir().unwrap(), first_dir);
    }

    #[test]
    #[cfg_attr(
        miri,
        ignore = "Miri cannot emulate this test's real directory creation and chdir calls"
    )]
    fn ctx_switch_keep_cwd_reapplies_autochdir() {
        let _lock = crate::globals::global_state_test_lock();
        let _cwd_lock = crate::os::fs::cwd_test_lock();
        let cwd = TempCwdGuard::new();
        let first_dir = std::env::current_dir().unwrap();
        let second_dir = cwd.root.join("two");
        let _g = CurwinGuard::save();
        let _acd = AutochdirGuard::set(1);
        let mut full_name = first_dir
            .join("file.txt")
            .to_string_lossy()
            .into_owned()
            .into_bytes();
        crate::path::path_to_slash(&mut full_name);
        let mut first_buf = crate::buffer_defs::BufT {
            b_ffname: Some(full_name.clone()),
            b_fname: Some(full_name.clone()),
            b_sfname: Some(full_name),
            ..Default::default()
        };
        let first_buf_ptr = std::ptr::addr_of_mut!(first_buf);
        let mut second_buf = crate::buffer_defs::BufT::default();
        let second_buf_ptr = std::ptr::addr_of_mut!(second_buf);
        let mut second = crate::buffer_defs::WinT {
            handle: 42,
            w_buffer: second_buf_ptr,
            ..Default::default()
        };
        let second_ptr = std::ptr::addr_of_mut!(second);
        let mut first = crate::buffer_defs::WinT {
            handle: 41,
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
        }
        let _firstbuf = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.firstbuf,
                first_buf_ptr,
            )
        };
        let _lastbuf = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.lastbuf,
                first_buf_ptr,
            )
        };
        let _starting = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.starting,
                0,
            )
        };
        let mut switch = CtxSwitch::default();
        assert!(unsafe {
            ctx_switch(
                &mut switch,
                second_ptr,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                crate::context_defs::ctx_switch_flags::KEEP_CWD,
            )
        });
        assert!(switch.cs_apply_acd);
        std::env::set_current_dir(&second_dir).unwrap();

        unsafe { ctx_restore(&switch) };

        assert_eq!(std::env::current_dir().unwrap(), first_dir);
    }

    #[test]
    fn ctx_switch_prepares_and_releases_an_autocmd_window() {
        let _lock = crate::globals::global_state_test_lock();
        let _pool = CtxWinPoolGuard::reset();
        let _g = CurwinGuard::save();
        let mut current_buf = crate::buffer_defs::BufT::default();
        let current_buf_ptr = std::ptr::addr_of_mut!(current_buf);
        let mut target_buf = crate::buffer_defs::BufT::default();
        let target_buf_ptr = std::ptr::addr_of_mut!(target_buf);
        let mut current = crate::buffer_defs::WinT {
            handle: 51,
            w_buffer: current_buf_ptr,
            ..Default::default()
        };
        let current_ptr = std::ptr::addr_of_mut!(current);
        let mut tab = crate::buffer_defs::TabpageT {
            tp_curwin: current_ptr,
            tp_firstwin: current_ptr,
            tp_lastwin: current_ptr,
            tp_localdir: Some(b"tab-local".to_vec()),
            ..Default::default()
        };
        let tab_ptr = std::ptr::addr_of_mut!(tab);
        {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            globals.firstwin = current_ptr;
            globals.curwin = current_ptr;
            globals.curbuf = current_buf_ptr;
        }
        let _lastwin = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.lastwin,
                current_ptr,
            )
        };
        let _curtab = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.curtab,
                tab_ptr,
            )
        };
        let _firsttab = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.first_tabpage,
                tab_ptr,
            )
        };
        let _globaldir =
            GlobaldirGuard::set(Some(b"global-dir".to_vec()));
        let mut switch = CtxSwitch::default();

        assert!(unsafe {
            ctx_switch(
                &mut switch,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                target_buf_ptr,
                crate::context_defs::ctx_switch_flags::NO_EVENTS,
            )
        });

        let temp = unsafe { crate::globals::GLOBALS.get_mut() }.curwin;
        assert!(is_ctx_win(temp));
        assert_eq!(unsafe { (*temp).w_buffer }, target_buf_ptr);
        assert_eq!(unsafe { (*target_buf_ptr).b_nwindows }, 1);
        assert_eq!(unsafe { (*current_ptr).w_next }, temp);
        assert!(switch.cs_ctxwin_idx >= 0);
        assert!(unsafe { (*tab_ptr).tp_localdir.is_none() });
        assert!(unsafe { crate::globals::GLOBALS.get_mut() }.globaldir.is_none());

        unsafe { ctx_restore(&switch) };

        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.curwin, current_ptr);
        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.curbuf, current_buf_ptr);
        assert_eq!(unsafe { (*target_buf_ptr).b_nwindows }, 0);
        assert!(unsafe { (*current_ptr).w_next }.is_null());
        assert!(!is_ctx_win(temp));
        assert_eq!(
            unsafe { (*tab_ptr).tp_localdir.as_deref() },
            Some(b"tab-local".as_slice())
        );
        assert_eq!(
            unsafe { crate::globals::GLOBALS.get_mut() }.globaldir.as_deref(),
            Some(b"global-dir".as_slice())
        );
        assert!(!crate::autocmd::is_autocmd_blocked());
    }

    #[test]
    fn option_write_uses_an_autocmd_window_for_an_unshown_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let _pool = CtxWinPoolGuard::reset();
        let _g = CurwinGuard::save();
        let mut current_buf = crate::buffer_defs::BufT::default();
        let current_buf_ptr = std::ptr::addr_of_mut!(current_buf);
        let mut target_buf = crate::buffer_defs::BufT {
            b_p_ts: 8,
            ..Default::default()
        };
        let target_buf_ptr = std::ptr::addr_of_mut!(target_buf);
        let mut current = crate::buffer_defs::WinT {
            handle: 61,
            w_buffer: current_buf_ptr,
            ..Default::default()
        };
        let current_ptr = std::ptr::addr_of_mut!(current);
        let mut tab = crate::buffer_defs::TabpageT {
            tp_curwin: current_ptr,
            tp_firstwin: current_ptr,
            tp_lastwin: current_ptr,
            ..Default::default()
        };
        let tab_ptr = std::ptr::addr_of_mut!(tab);
        {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            globals.firstwin = current_ptr;
            globals.lastwin = current_ptr;
            globals.curwin = current_ptr;
            globals.curbuf = current_buf_ptr;
            globals.curtab = tab_ptr;
            globals.first_tabpage = tab_ptr;
        }

        assert_eq!(
            unsafe {
                crate::option::set_option_value_for(
                    b"tabstop",
                    crate::option_defs::OptIndex::Tabstop,
                    crate::option_defs::OptVal::Number(3),
                    crate::option_defs::opt_set_flags::OPT_LOCAL,
                    crate::option_defs::OptScope::Buf,
                    target_buf_ptr.cast(),
                )
            },
            None
        );
        assert_eq!(unsafe { (*target_buf_ptr).b_p_ts }, 3);
        assert_eq!(unsafe { (*target_buf_ptr).b_nwindows }, 0);
        assert_eq!(
            unsafe { crate::globals::GLOBALS.get_mut() }.curwin,
            current_ptr
        );
        assert_eq!(
            unsafe { crate::globals::GLOBALS.get_mut() }.curbuf,
            current_buf_ptr
        );
        assert!(
            unsafe { CTX_WIN_VEC.get_mut() }
                .iter()
                .all(|entry| !entry.cw_used)
        );
    }

    #[test]
    fn ctx_restore_is_a_noop_for_a_default_zeroed_ctx_switch() {
        let cs = CtxSwitch::default();
        unsafe { ctx_restore(&cs) }; // must not panic
    }

    #[test]
    fn is_ctx_win_false_when_pool_is_empty() {
        let _lock = crate::globals::global_state_test_lock();
        let _pool = CtxWinPoolGuard::reset();
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
