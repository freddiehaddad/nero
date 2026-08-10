//! Translated from `src/nvim/api/tabpage.c` (tractable subset only -
//! most of this file needs `Object`-conversion machinery, `Dict`-
//! scoped-variable access, and `win_goto`'s real window-switching
//! machinery, none of which are wired into this API layer yet).
//!
//! Translated: [`nvim_tabpage_get_win`], [`nvim_tabpage_get_number`],
//! [`nvim_tabpage_is_valid`].
//!
//! Deferred (each needs a real, not-yet-translated subsystem beyond
//! `find_tab_by_handle` itself): `nvim_tabpage_get/set/del_var`
//! (`dict_get_value`/`dict_set_var`, the API layer's `Object`-to-
//! `typval_T` bridge), `nvim_tabpage_set_win` (needs `win_goto`'s
//! real window-switching machinery for the "switching to the current
//! tabpage" branch), `nvim_tabpage_list_wins` (needs `Array`
//! conversion).

use crate::api::private::defs::{Boolean, Error, Integer, Tabpage, Window};
use crate::api::private::helpers::find_tab_by_handle;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer_defs::{TabpageT, WinT};

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
