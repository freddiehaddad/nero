//! Window-related Vimscript helpers from `src/nvim/eval/window.c`.
//!
//! These helpers predate this file and remain implemented in
//! `crate::window`; re-export them here so the Rust module tree mirrors
//! their actual Neovim source file without duplicating behavior.

pub use crate::window::{
    find_tabwin, find_win_by_nr, find_win_by_nr_or_id, get_winnr,
    win_findbuf, win_getid, win_has_winnr, win_id2win, win_id2wp,
    win_id2wp_tp,
};

/// Build the dictionary returned for one window by `getwininfo()`
/// (`get_win_info`).
///
/// # Safety
/// `window` and its buffer/variable dictionary pointers must be valid
/// live objects; forwards botline and column-offset requirements.
#[must_use]
pub unsafe fn get_win_info(
    window: *mut crate::buffer_defs::WinT,
    tabnr: i16,
    winnr: i16,
) -> *mut crate::eval::typval_defs::DictT {
    assert!(!window.is_null());
    unsafe { crate::r#move::validate_botline_win(window) };

    let dictionary = crate::eval::typval::tv_dict_alloc();
    let dict = unsafe { &mut *dictionary };
    let win = unsafe { &mut *window };
    assert!(!win.w_buffer.is_null());
    let buffer = unsafe { &*win.w_buffer };

    let values: &[(&[u8], i64)] = &[
        (b"tabnr", i64::from(tabnr)),
        (b"winnr", i64::from(winnr)),
        (b"winid", i64::from(win.handle)),
        (b"height", i64::from(win.w_view_height)),
        (b"status_height", i64::from(win.w_status_height)),
        (b"winrow", i64::from(win.w_winrow + 1)),
        (b"topline", i64::from(win.w_topline)),
        (b"botline", i64::from(win.w_botline - 1)),
        (b"leftcol", i64::from(win.w_leftcol)),
        (b"winbar", i64::from(win.w_winbar_height)),
        (b"width", i64::from(win.w_view_width)),
        (b"bufnr", i64::from(buffer.handle)),
        (b"wincol", i64::from(win.w_wincol + 1)),
        (
            b"textoff",
            i64::from(unsafe { crate::r#move::win_col_off(win) }),
        ),
        (
            b"terminal",
            i64::from(crate::buffer::bt_terminal(Some(buffer))),
        ),
        (
            b"quickfix",
            i64::from(crate::buffer::bt_quickfix(Some(buffer))),
        ),
        (
            b"loclist",
            i64::from(
                crate::buffer::bt_quickfix(Some(buffer))
                    && !win.w_llist_ref.is_null(),
            ),
        ),
    ];
    for &(key, value) in values {
        crate::eval::typval::tv_dict_add_nr(dict, key, value);
    }
    unsafe {
        crate::eval::typval::tv_dict_add_dict(
            dict,
            b"variables",
            win.w_vars,
        );
    }
    dictionary
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe fn number(
        dictionary: *mut crate::eval::typval_defs::DictT,
        key: &[u8],
    ) -> i64 {
        let item = unsafe {
            crate::eval::typval::tv_dict_find(
                Some(&mut *dictionary),
                key,
            )
        }
        .expect("dictionary key");
        let crate::eval::typval_defs::TypvalValue::Number(value) =
            (unsafe { &(*item).di_tv.value })
        else {
            panic!("expected number");
        };
        *value
    }

    #[test]
    fn get_win_info_reports_window_geometry_and_identity() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buffer = crate::buffer_defs::BufT {
            handle: 42,
            ..Default::default()
        };
        let variables = crate::eval::typval::tv_dict_alloc();
        unsafe { (*variables).dv_refcount = 1 };
        let mut window = crate::buffer_defs::WinT {
            handle: 77,
            w_buffer: &mut buffer,
            w_vars: variables,
            w_view_height: 10,
            w_view_width: 40,
            w_status_height: 1,
            w_winbar_height: 1,
            w_winrow: 2,
            w_wincol: 3,
            w_topline: 4,
            w_botline: 9,
            w_leftcol: 5,
            w_valid: i32::from(
                crate::buffer_defs::w_valid::VALID_BOTLINE,
            ),
            ..Default::default()
        };

        let dictionary =
            unsafe { get_win_info(&mut window, 2, 3) };
        assert_eq!(unsafe { number(dictionary, b"tabnr") }, 2);
        assert_eq!(unsafe { number(dictionary, b"winnr") }, 3);
        assert_eq!(unsafe { number(dictionary, b"winid") }, 77);
        assert_eq!(unsafe { number(dictionary, b"height") }, 10);
        assert_eq!(unsafe { number(dictionary, b"width") }, 40);
        assert_eq!(unsafe { number(dictionary, b"winrow") }, 3);
        assert_eq!(unsafe { number(dictionary, b"wincol") }, 4);
        assert_eq!(unsafe { number(dictionary, b"topline") }, 4);
        assert_eq!(unsafe { number(dictionary, b"botline") }, 8);
        assert_eq!(unsafe { number(dictionary, b"leftcol") }, 5);
        assert_eq!(unsafe { number(dictionary, b"bufnr") }, 42);

        let item = unsafe {
            crate::eval::typval::tv_dict_find(
                Some(&mut *dictionary),
                b"variables",
            )
        }
        .unwrap();
        assert!(matches!(
            unsafe { &(*item).di_tv.value },
            crate::eval::typval_defs::TypvalValue::Dict(dict)
                if *dict == variables
        ));

        unsafe {
            crate::eval::typval::tv_dict_free(dictionary);
            crate::eval::typval::tv_dict_unref(variables);
        }
    }
}
