//! Translated from `src/nvim/api/tabpage.c` (tractable subset only -
//! most of this file needs `Object`-conversion machinery, `Dict`-
//! scoped-variable access, and `win_goto`'s real window-switching
//! machinery, none of which are wired into this API layer yet).
//!
//! Translated: [`nvim_tabpage_list_wins`], [`nvim_tabpage_get_win`],
//! [`nvim_tabpage_get_number`], [`nvim_tabpage_get_var`],
//! [`nvim_tabpage_set_win`], [`nvim_tabpage_is_valid`].
//!
//! Deferred: [`nvim_tabpage_set_win`] is complete for non-current
//! tabpages and for idempotently selecting the current tab's existing
//! current window. Switching the current tab to a different window
//! still needs `win_goto`'s real window-switching machinery.

use crate::api::private::defs::{
    Array, Boolean, Error, Integer, NvimString, Object, Tabpage, Window,
};
use crate::api::private::helpers::find_tab_by_handle;
use crate::api::private::helpers::find_window_by_handle;

/// List every window in `tabpage` (`nvim_tabpage_list_wins`).
///
/// # Safety
/// Forwarded from [`find_tab_by_handle`]; the selected tabpage's
/// window list must consist of live pointers.
#[must_use]
pub unsafe fn nvim_tabpage_list_wins(tabpage: Tabpage, err: &mut Error) -> Array {
    let tab = unsafe { find_tab_by_handle(tabpage, err) };
    if tab.is_null() || !unsafe { crate::window::valid_tabpage(tab) } {
        return Vec::new();
    }
    let mut win = if std::ptr::eq(tab, unsafe { crate::globals::GLOBALS.get_mut() }.curtab) {
        unsafe { crate::globals::GLOBALS.get_mut() }.firstwin
    } else {
        unsafe { (*tab).tp_firstwin }
    };
    let mut result = Vec::new();
    while !win.is_null() {
        result.push(Object::Window(unsafe { (*win).handle }));
        win = unsafe { (*win).w_next };
    }
    result
}

/// Get the handle of the current window in tab page `tabpage` (`0`
/// for the current tabpage), or `0` on failure
/// (`nvim_tabpage_get_win`).
///
/// # Safety
/// Forwarded from [`find_tab_by_handle`]'s own safety doc; also
/// requires `tab`'s own `tp_firstwin`/`w_next` window list (when
/// `tab` isn't the current tabpage) to consist of valid, live
/// pointers, and `tab.tp_curwin` to genuinely be a member of that
/// list (matching the original's own "there should always be a
/// current window for a tabpage" invariant, backed there by an
/// unconditional `abort()` - translated the same way here via
/// `unreachable!()`, since both fire regardless of build profile).
pub unsafe fn nvim_tabpage_get_win(tabpage: Tabpage, err: &mut Error) -> Window {
    // SAFETY: forwarded from this function's own safety doc.
    let tab = unsafe { find_tab_by_handle(tabpage, err) };
    // SAFETY: forwarded from this function's own safety doc.
    if tab.is_null() || !unsafe { crate::window::valid_tabpage(tab) } {
        return 0;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let curtab = unsafe { crate::globals::GLOBALS.get_mut() }.curtab;
    if std::ptr::eq(tab, curtab) {
        // SAFETY: forwarded from this function's own safety doc.
        return unsafe { crate::api::vim::nvim_get_current_win() };
    }

    // SAFETY: forwarded from this function's own safety doc.
    let tp_curwin = unsafe { &*tab }.tp_curwin;
    // SAFETY: forwarded from this function's own safety doc.
    let mut wp = unsafe { &*tab }.tp_firstwin;
    while !wp.is_null() {
        if std::ptr::eq(wp, tp_curwin) {
            // SAFETY: forwarded from this function's own safety doc.
            return unsafe { &*wp }.handle;
        }
        // SAFETY: forwarded from this function's own safety doc.
        wp = unsafe { &*wp }.w_next;
    }
    unreachable!(
        "nvim_tabpage_get_win: tab.tp_curwin was not a member of tab's own window list - \
         there should always be a current window for a tabpage"
    );
}

/// Set the current window in `tabpage` (`nvim_tabpage_set_win`).
///
/// # Safety
/// Forwarded from the handle lookup and tabpage window-list helpers.
pub unsafe fn nvim_tabpage_set_win(
    tabpage: Tabpage,
    win: Window,
    err: &mut Error,
) {
    let tab = unsafe { find_tab_by_handle(tabpage, err) };
    if tab.is_null() {
        return;
    }
    let window = unsafe { find_window_by_handle(win, err) };
    if window.is_null() {
        return;
    }
    if !unsafe { crate::window::tabpage_win_valid(tab, window) } {
        err.r#type = crate::api::private::defs::ErrorType::Exception;
        err.msg = Some(format!(
            "Window does not belong to tabpage {}",
            unsafe { (*tab).handle }
        ));
        return;
    }

    if std::ptr::eq(tab, unsafe { crate::globals::GLOBALS.get_mut() }.curtab) {
        if std::ptr::eq(
            window,
            unsafe { crate::globals::GLOBALS.get_mut() }.curwin,
        ) {
            return;
        }
        unimplemented!(
            "nvim_tabpage_set_win: current-tab switching needs win_goto"
        );
    }
    if !std::ptr::eq(unsafe { (*tab).tp_curwin }, window) {
        unsafe {
            (*tab).tp_prevwin = (*tab).tp_curwin;
            (*tab).tp_curwin = window;
        }
    }
}

/// Get the 1-based tabpage number of `tabpage` (`0` for the current
/// tabpage), or `0` on failure (`nvim_tabpage_get_number`).
///
/// # Safety
/// Forwarded from [`find_tab_by_handle`]'s own safety doc.
pub unsafe fn nvim_tabpage_get_number(tabpage: Tabpage, err: &mut Error) -> Integer {
    // SAFETY: forwarded from this function's own safety doc.
    let tab = unsafe { find_tab_by_handle(tabpage, err) };
    if tab.is_null() {
        return 0;
    }
    // SAFETY: forwarded from this function's own safety doc.
    i64::from(unsafe { crate::window::tabpage_index(tab) })
}

/// Whether `tabpage` (`0` for the current tabpage) refers to a
/// currently valid tab page (`nvim_tabpage_is_valid`).
///
/// # Safety
/// Forwarded from [`find_tab_by_handle`]'s own safety doc.
#[must_use]
pub unsafe fn nvim_tabpage_is_valid(tabpage: Tabpage) -> Boolean {
    let mut stub = Error::default();
    // SAFETY: forwarded from this function's own safety doc.
    !unsafe { find_tab_by_handle(tabpage, &mut stub) }.is_null()
}

/// Get a tabpage-scoped variable (`nvim_tabpage_get_var`).
///
/// # Safety
/// Forwarded from [`find_tab_by_handle`] and the scope-dictionary
/// converter.
pub unsafe fn nvim_tabpage_get_var(
    tabpage: Tabpage,
    name: &NvimString,
    err: &mut Error,
) -> Object {
    let tabpage = unsafe { find_tab_by_handle(tabpage, err) };
    if tabpage.is_null() {
        return Object::Nil;
    }
    unsafe {
        crate::api::private::helpers::dict_get_value((*tabpage).tp_vars, name, err)
    }
}

/// Set a tabpage-scoped variable (`nvim_tabpage_set_var`).
///
/// # Safety
/// Forwarded from [`find_tab_by_handle`] and the checked
/// scope-dictionary writer.
pub unsafe fn nvim_tabpage_set_var(
    tabpage: Tabpage,
    name: &NvimString,
    value: &Object,
    err: &mut Error,
) {
    let tabpage = unsafe { find_tab_by_handle(tabpage, err) };
    if tabpage.is_null() {
        return;
    }
    let _ = unsafe {
        crate::api::private::helpers::dict_set_var(
            (*tabpage).tp_vars,
            name,
            value,
            false,
            false,
            err,
        )
    };
}

/// Delete a tabpage-scoped variable (`nvim_tabpage_del_var`).
///
/// # Safety
/// Forwarded from [`find_tab_by_handle`] and the checked
/// scope-dictionary writer.
pub unsafe fn nvim_tabpage_del_var(
    tabpage: Tabpage,
    name: &NvimString,
    err: &mut Error,
) {
    let tabpage = unsafe { find_tab_by_handle(tabpage, err) };
    if tabpage.is_null() {
        return;
    }
    let _ = unsafe {
        crate::api::private::helpers::dict_set_var(
            (*tabpage).tp_vars,
            name,
            &Object::Nil,
            true,
            false,
            err,
        )
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer_defs::{TabpageT, WinT};

    #[test]
    fn nvim_tabpage_get_var_returns_a_real_tabpage_variable() {
        let _lock = crate::globals::global_state_test_lock();
        let mut fx = TabFixture::new(31);
        let dict = crate::eval::typval::tv_dict_alloc();
        assert_eq!(
            unsafe { crate::eval::typval::tv_dict_add_nr(&mut *dict, b"answer", 42) },
            crate::vim_defs::OK
        );
        fx.tab_mut().tp_vars = dict;
        let mut err = Error::default();
        let value = unsafe {
            nvim_tabpage_get_var(fx.handle(), &b"answer".to_vec(), &mut err)
        };
        assert!(matches!(value, Object::Integer(42)));
        assert!(!err.is_set());
        fx.tab_mut().tp_vars = std::ptr::null_mut();
        unsafe { crate::eval::typval::tv_dict_unref(dict) };
    }

    #[test]
    fn nvim_tabpage_set_var_stores_a_tabpage_variable() {
        let _lock = crate::globals::global_state_test_lock();
        let mut fx = TabFixture::new(32);
        let dict = crate::eval::typval::tv_dict_alloc();
        fx.tab_mut().tp_vars = dict;
        let mut err = Error::default();
        unsafe {
            nvim_tabpage_set_var(
                fx.handle(),
                &b"value".to_vec(),
                &Object::Integer(7),
                &mut err,
            )
        };
        let value =
            unsafe { nvim_tabpage_get_var(fx.handle(), &b"value".to_vec(), &mut err) };
        assert!(matches!(value, Object::Integer(7)));
        assert!(!err.is_set());
        let item = unsafe { crate::eval::typval::tv_dict_find(Some(&mut *dict), b"value") }
            .expect("tabpage variable");
        unsafe { crate::eval::typval::tv_dict_item_remove(&mut *dict, item) };
        fx.tab_mut().tp_vars = std::ptr::null_mut();
        unsafe { crate::eval::typval::tv_dict_unref(dict) };
    }

    #[test]
    fn nvim_tabpage_del_var_removes_a_tabpage_variable() {
        let _lock = crate::globals::global_state_test_lock();
        let mut fx = TabFixture::new(33);
        let dict = crate::eval::typval::tv_dict_alloc();
        assert_eq!(
            unsafe { crate::eval::typval::tv_dict_add_nr(&mut *dict, b"value", 3) },
            crate::vim_defs::OK
        );
        fx.tab_mut().tp_vars = dict;
        let mut err = Error::default();
        unsafe {
            nvim_tabpage_del_var(fx.handle(), &b"value".to_vec(), &mut err)
        };
        assert!(unsafe { crate::eval::typval::tv_dict_find(Some(&mut *dict), b"value") }.is_none());
        assert!(!err.is_set());
        fx.tab_mut().tp_vars = std::ptr::null_mut();
        unsafe { crate::eval::typval::tv_dict_unref(dict) };
    }

    /// `Box::into_raw`/`Box::from_raw` (never a live `Box` field
    /// alongside a separately-derived raw pointer) - see
    /// `api/buffer.rs`'s own `BufFixture` doc comment for the full
    /// Tree Borrows reasoning this mirrors. `curtab` is left
    /// untouched by `new` itself - each test sets it explicitly,
    /// since some tests need `tab_ptr != curtab` and others need
    /// `tab_ptr == curtab`.
    struct TabFixture {
        tab_ptr: *mut TabpageT,
        prev_first_tabpage: *mut TabpageT,
        prev_curtab: *mut TabpageT,
    }

    impl TabFixture {
        fn new(handle: crate::types_defs::HandleT) -> Self {
            let tab_ptr = Box::into_raw(Box::new(TabpageT { handle, ..Default::default() }));

            // SAFETY: single-threaded test, GLOBALS restored in Drop.
            unsafe {
                let g = crate::globals::GLOBALS.get_mut();
                let prev_first_tabpage = g.first_tabpage;
                let prev_curtab = g.curtab;
                g.first_tabpage = tab_ptr;
                TabFixture { tab_ptr, prev_first_tabpage, prev_curtab }
            }
        }

        fn tab_mut(&mut self) -> &mut TabpageT {
            // SAFETY: `tab_ptr` was allocated in `new` and stays
            // valid until this fixture's own `Drop` runs.
            unsafe { &mut *self.tab_ptr }
        }

        fn handle(&self) -> crate::types_defs::HandleT {
            // SAFETY: same as `tab_mut`'s own doc.
            unsafe { (*self.tab_ptr).handle }
        }
    }

    impl Drop for TabFixture {
        fn drop(&mut self) {
            // SAFETY: restoring exactly what `new` overwrote, then
            // reclaiming the `Box` allocated via `Box::into_raw` in
            // `new` - the only reconstruction of a `Box` over this
            // pointer, so there is no sibling-reborrow conflict.
            unsafe {
                let g = crate::globals::GLOBALS.get_mut();
                g.first_tabpage = self.prev_first_tabpage;
                g.curtab = self.prev_curtab;
                drop(Box::from_raw(self.tab_ptr));
            }
        }
    }

    /// `nvim_tabpage_get_win`'s own `tab == curtab` branch delegates
    /// to `nvim_get_current_win` - it never even looks at `tab`'s own
    /// `tp_firstwin`/`tp_curwin` fields at all.
    #[test]
    fn nvim_tabpage_get_win_uses_current_win_for_curtab() {
        let _lock = crate::globals::global_state_test_lock();
        let fx = TabFixture::new(5);
        let tab_ptr = fx.tab_ptr;
        // SAFETY: single-threaded test, restored by `fx`'s own Drop.
        unsafe { crate::globals::GLOBALS.get_mut().curtab = tab_ptr };

        let mut win = WinT { handle: 77, ..Default::default() };
        let win_ptr = std::ptr::addr_of_mut!(win);
        // Guarded rather than restored by hand: `win` is a local, so
        // an assertion failure below would otherwise leave `curwin`
        // dangling for whatever test runs next.
        // SAFETY: single-threaded test holding the global state lock.
        let _cw = unsafe {
            crate::globals::GlobalFieldGuard::install(|g| &mut g.curwin, win_ptr)
        };

        let handle = fx.handle();
        let mut err = Error::default();
        // SAFETY: `fx`/`win` set up a valid GLOBALS.first_tabpage/
        // curtab/curwin.
        let win_handle = unsafe { nvim_tabpage_get_win(handle, &mut err) };

        assert_eq!(win_handle, 77);
        assert!(!err.is_set());
    }

    #[test]
    fn nvim_tabpage_list_wins_returns_every_current_tab_window() {
        let _lock = crate::globals::global_state_test_lock();
        let mut second = WinT {
            handle: 12,
            ..Default::default()
        };
        let second_ptr = std::ptr::addr_of_mut!(second);
        let mut first = WinT {
            handle: 11,
            w_next: second_ptr,
            ..Default::default()
        };
        let first_ptr = std::ptr::addr_of_mut!(first);
        let fx = TabFixture::new(19);
        unsafe { crate::globals::GLOBALS.get_mut() }.curtab = fx.tab_ptr;
        let _firstwin = unsafe {
            crate::globals::GlobalFieldGuard::install(|g| &mut g.firstwin, first_ptr)
        };
        let mut err = Error::default();

        let windows = unsafe { nvim_tabpage_list_wins(fx.handle(), &mut err) };

        assert!(!err.is_set());
        assert!(matches!(
            windows.as_slice(),
            [Object::Window(11), Object::Window(12)]
        ));
    }

    #[test]
    fn nvim_tabpage_list_wins_returns_empty_for_an_unknown_tabpage() {
        let _lock = crate::globals::global_state_test_lock();
        let _fx = TabFixture::new(20);
        let mut err = Error::default();
        assert!(unsafe { nvim_tabpage_list_wins(99, &mut err) }.is_empty());
        assert_eq!(err.msg.as_deref(), Some("Invalid tabpage id: 99"));
    }

    /// For a NON-current tabpage, `nvim_tabpage_get_win` walks
    /// `tab.tp_firstwin`/`w_next` looking for `tab.tp_curwin`.
    #[test]
    fn nvim_tabpage_get_win_finds_tp_curwin_for_a_non_curtab() {
        let _lock = crate::globals::global_state_test_lock();
        let mut second_win = WinT { handle: 11, ..Default::default() };
        let second_ptr = std::ptr::addr_of_mut!(second_win);
        let mut first_win =
            WinT { handle: 9, w_next: second_ptr, ..Default::default() };
        let first_ptr = std::ptr::addr_of_mut!(first_win);

        let mut fx = TabFixture::new(6);
        fx.tab_mut().tp_firstwin = first_ptr;
        fx.tab_mut().tp_curwin = second_ptr;
        // A distinct, unrelated tabpage as curtab - not the same as
        // `fx`'s own tabpage.
        // SAFETY: single-threaded test, restored by `fx`'s own Drop.
        unsafe { crate::globals::GLOBALS.get_mut().curtab = std::ptr::null_mut() };

        let handle = fx.handle();
        let mut err = Error::default();
        // SAFETY: `fx` sets up a valid GLOBALS.first_tabpage, and its
        // own tp_firstwin/tp_curwin chain is a valid, live window
        // list.
        let win_handle = unsafe { nvim_tabpage_get_win(handle, &mut err) };

        assert_eq!(win_handle, 11);
        assert!(!err.is_set());
    }

    #[test]
    fn nvim_tabpage_set_win_updates_a_noncurrent_tabpage() {
        let _lock = crate::globals::global_state_test_lock();
        let mut old_win = WinT { handle: 21, ..Default::default() };
        let old_ptr = std::ptr::addr_of_mut!(old_win);
        let mut new_win = WinT { handle: 22, ..Default::default() };
        let new_ptr = std::ptr::addr_of_mut!(new_win);
        unsafe { (*old_ptr).w_next = new_ptr };
        let fx = TabFixture::new(11);
        unsafe {
            (*fx.tab_ptr).tp_firstwin = old_ptr;
            (*fx.tab_ptr).tp_lastwin = new_ptr;
            (*fx.tab_ptr).tp_curwin = old_ptr;
            crate::globals::GLOBALS.get_mut().curtab = std::ptr::null_mut();
        }
        let mut err = Error::default();

        unsafe { nvim_tabpage_set_win(fx.handle(), 22, &mut err) };

        assert!(!err.is_set());
        assert_eq!(unsafe { (*fx.tab_ptr).tp_prevwin }, old_ptr);
        assert_eq!(unsafe { (*fx.tab_ptr).tp_curwin }, new_ptr);
    }

    #[test]
    fn nvim_tabpage_set_win_keeps_prevwin_when_already_current() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT { handle: 23, ..Default::default() };
        let win_ptr = std::ptr::addr_of_mut!(win);
        let fx = TabFixture::new(12);
        unsafe {
            (*fx.tab_ptr).tp_firstwin = win_ptr;
            (*fx.tab_ptr).tp_lastwin = win_ptr;
            (*fx.tab_ptr).tp_curwin = win_ptr;
            crate::globals::GLOBALS.get_mut().curtab = std::ptr::null_mut();
        }
        let mut err = Error::default();

        unsafe { nvim_tabpage_set_win(fx.handle(), 23, &mut err) };

        assert!(!err.is_set());
        assert!(unsafe { (*fx.tab_ptr).tp_prevwin }.is_null());
        assert_eq!(unsafe { (*fx.tab_ptr).tp_curwin }, win_ptr);
    }

    #[test]
    fn nvim_tabpage_set_win_accepts_the_current_tabs_current_window() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT {
            handle: 34,
            ..Default::default()
        };
        let win_ptr = std::ptr::addr_of_mut!(win);
        let fx = TabFixture::new(34);
        unsafe {
            (*fx.tab_ptr).tp_firstwin = win_ptr;
            (*fx.tab_ptr).tp_lastwin = win_ptr;
            (*fx.tab_ptr).tp_curwin = win_ptr;
        }
        let _curtab = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.curtab,
                fx.tab_ptr,
            )
        };
        let _curwin = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.curwin,
                win_ptr,
            )
        };
        let _firstwin = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.firstwin,
                win_ptr,
            )
        };
        let mut err = Error::default();

        unsafe { nvim_tabpage_set_win(fx.handle(), 34, &mut err) };

        assert!(!err.is_set());
        assert_eq!(unsafe { (*fx.tab_ptr).tp_curwin }, win_ptr);
        assert!(unsafe { (*fx.tab_ptr).tp_prevwin }.is_null());
    }

    #[test]
    fn nvim_tabpage_set_win_rejects_a_window_from_another_tabpage() {
        let _lock = crate::globals::global_state_test_lock();
        let mut target_win = WinT { handle: 24, ..Default::default() };
        let target_win_ptr = std::ptr::addr_of_mut!(target_win);
        let mut foreign_win = WinT { handle: 25, ..Default::default() };
        let foreign_win_ptr = std::ptr::addr_of_mut!(foreign_win);
        let fx = TabFixture::new(13);
        let foreign_tab = Box::into_raw(Box::new(TabpageT {
            handle: 14,
            tp_firstwin: foreign_win_ptr,
            tp_lastwin: foreign_win_ptr,
            tp_curwin: foreign_win_ptr,
            ..Default::default()
        }));
        unsafe {
            (*fx.tab_ptr).tp_firstwin = target_win_ptr;
            (*fx.tab_ptr).tp_lastwin = target_win_ptr;
            (*fx.tab_ptr).tp_curwin = target_win_ptr;
            (*fx.tab_ptr).tp_next = foreign_tab;
            crate::globals::GLOBALS.get_mut().curtab = std::ptr::null_mut();
        }
        let mut err = Error::default();

        unsafe { nvim_tabpage_set_win(fx.handle(), 25, &mut err) };

        assert_eq!(
            err.msg.as_deref(),
            Some("Window does not belong to tabpage 13")
        );
        assert_eq!(unsafe { (*fx.tab_ptr).tp_curwin }, target_win_ptr);
        unsafe {
            (*fx.tab_ptr).tp_next = std::ptr::null_mut();
            drop(Box::from_raw(foreign_tab));
        }
    }

    #[test]
    fn nvim_tabpage_get_win_zero_for_an_unknown_handle() {
        let _lock = crate::globals::global_state_test_lock();
        let _fx = TabFixture::new(7);
        let mut err = Error::default();
        // SAFETY: `_fx` sets up a valid GLOBALS.first_tabpage, but
        // `9999` is a genuinely unrecognized handle.
        let win_handle = unsafe { nvim_tabpage_get_win(9999, &mut err) };
        assert_eq!(win_handle, 0);
        assert!(err.is_set());
    }

    #[test]
    fn nvim_tabpage_get_number_returns_the_1_based_position() {
        let _lock = crate::globals::global_state_test_lock();
        let fx = TabFixture::new(8);
        let handle = fx.handle();
        let mut err = Error::default();
        // SAFETY: `fx` sets up a valid GLOBALS.first_tabpage, and is
        // itself the ONLY (thus first) tabpage in that list.
        let number = unsafe { nvim_tabpage_get_number(handle, &mut err) };
        assert_eq!(number, 1);
        assert!(!err.is_set());
    }

    #[test]
    fn nvim_tabpage_get_number_zero_for_an_unknown_handle() {
        let _lock = crate::globals::global_state_test_lock();
        let _fx = TabFixture::new(9);
        let mut err = Error::default();
        // SAFETY: `_fx` sets up a valid GLOBALS.first_tabpage, but
        // `9999` is a genuinely unrecognized handle.
        let number = unsafe { nvim_tabpage_get_number(9999, &mut err) };
        assert_eq!(number, 0);
        assert!(err.is_set());
    }

    #[test]
    fn nvim_tabpage_is_valid_true_for_a_real_handle_false_for_unknown() {
        let _lock = crate::globals::global_state_test_lock();
        let fx = TabFixture::new(10);
        let handle = fx.handle();
        // SAFETY: `fx` sets up a valid GLOBALS.first_tabpage.
        assert!(unsafe { nvim_tabpage_is_valid(handle) });
        // SAFETY: same as above.
        assert!(!unsafe { nvim_tabpage_is_valid(9999) });
    }
}
