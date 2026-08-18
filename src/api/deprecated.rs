//! Translated from `src/nvim/api/deprecated.c` (highlight lookup and
//! read-only buffer-line compatibility subsets).
//!
//! The deprecated highlight-by-ID/name wrappers are retained because
//! they are public API compatibility surfaces. Other functions in the
//! source file remain with their respective not-yet-translated API
//! subsystems.

use crate::api::private::defs::{
    Boolean, Buffer, Dict, Error, ErrorType, Integer, NvimString, Object, StringArray, Window,
};
use crate::option_defs::OptScope;

fn convert_index(index: Integer) -> Integer {
    if index < 0 {
        index - 1
    } else {
        index
    }
}

/// Return the buffer number, which equals its API handle
/// (`nvim_buf_get_number`).
///
/// # Safety
/// Forwarded from
/// [`crate::api::private::helpers::find_buffer_by_handle`].
pub unsafe fn nvim_buf_get_number(buffer: Buffer, err: &mut Error) -> Integer {
    let buf = unsafe { crate::api::private::helpers::find_buffer_by_handle(buffer, err) };
    if buf.is_null() {
        0
    } else {
        i64::from(unsafe { (*buf).handle })
    }
}

unsafe fn get_option_from(
    from: *mut std::ffi::c_void,
    scope: OptScope,
    name: &NvimString,
    err: &mut Error,
) -> Object {
    if name.is_empty() {
        err.r#type = ErrorType::Validation;
        err.msg = Some("Invalid option name: '<empty>'".to_string());
        return Object::Nil;
    }
    let opt_idx = crate::option::find_option(name);
    if opt_idx == crate::option_defs::OptIndex::Invalid {
        err.r#type = ErrorType::Validation;
        err.msg = Some(format!(
            "Invalid option name: '{}'",
            String::from_utf8_lossy(name)
        ));
        return Object::Nil;
    }
    let value = if crate::option::option_has_scope(opt_idx, scope) {
        unsafe {
            crate::option::get_option_value_for(
                opt_idx,
                if scope == OptScope::Global {
                    crate::option_defs::opt_set_flags::OPT_GLOBAL
                } else {
                    crate::option_defs::opt_set_flags::OPT_LOCAL
                },
                scope,
                from,
            )
        }
    } else {
        crate::option_defs::OptVal::Nil
    };
    if value == crate::option_defs::OptVal::Nil {
        err.r#type = ErrorType::Validation;
        err.msg = Some(format!(
            "Invalid option name: '{}'",
            String::from_utf8_lossy(name)
        ));
        Object::Nil
    } else {
        crate::option::optval_as_object(value)
    }
}

/// Get the global value of an option (`nvim_get_option`).
///
/// # Safety
/// Forwarded from [`crate::option::get_option_value_for`].
pub unsafe fn nvim_get_option(name: &NvimString, err: &mut Error) -> Object {
    unsafe { get_option_from(std::ptr::null_mut(), OptScope::Global, name, err) }
}

unsafe fn set_option_to(
    to: *mut std::ffi::c_void,
    scope: OptScope,
    name: &NvimString,
    value: Object,
    err: &mut Error,
) {
    if name.is_empty() {
        err.r#type = ErrorType::Validation;
        err.msg = Some("Invalid option name: '<empty>'".to_string());
        return;
    }
    let opt_idx = crate::option::find_option(name);
    if opt_idx == crate::option_defs::OptIndex::Invalid {
        err.r#type = ErrorType::Validation;
        err.msg = Some(format!(
            "Invalid option name: '{}'",
            String::from_utf8_lossy(name)
        ));
        return;
    }
    let actual_type = value.object_type();
    let (value, invalid_type) = crate::option::object_as_optval(value);
    if invalid_type {
        err.r#type = ErrorType::Validation;
        err.msg = Some(format!(
            "Invalid 'value': expected valid option type, got {actual_type:?}"
        ));
        return;
    }
    let opt_flags = if scope == OptScope::Win
        && !crate::option::option_has_scope(opt_idx, OptScope::Global)
    {
        0
    } else if scope == OptScope::Global {
        crate::option_defs::opt_set_flags::OPT_GLOBAL
    } else {
        crate::option_defs::opt_set_flags::OPT_LOCAL
    };
    if let Some(message) =
        unsafe { crate::option::set_option_value_for(name, opt_idx, value, opt_flags, scope, to) }
    {
        err.r#type = ErrorType::Validation;
        err.msg = Some(message.to_string());
    }
}

/// Set the global value of an option (`nvim_set_option`).
///
/// # Safety
/// Forwarded from [`crate::option::set_option_value_for`].
pub unsafe fn nvim_set_option(name: &NvimString, value: Object, err: &mut Error) {
    unsafe { set_option_to(std::ptr::null_mut(), OptScope::Global, name, value, err) };
}

/// Set a global variable and return its previous value (`vim_set_var`).
///
/// # Safety
/// Mutates the shared global-variable dictionary.
pub unsafe fn vim_set_var(name: &NvimString, value: &Object, err: &mut Error) -> Object {
    unsafe {
        crate::api::private::helpers::dict_set_var(
            crate::eval::vars::get_globvar_dict(),
            name,
            value,
            false,
            true,
            err,
        )
    }
}

/// Delete a global variable and return its previous value
/// (`vim_del_var`).
///
/// # Safety
/// Mutates the shared global-variable dictionary.
pub unsafe fn vim_del_var(name: &NvimString, err: &mut Error) -> Object {
    unsafe {
        crate::api::private::helpers::dict_set_var(
            crate::eval::vars::get_globvar_dict(),
            name,
            &Object::Nil,
            true,
            true,
            err,
        )
    }
}

/// Set a buffer variable and return its previous value
/// (`buffer_set_var`).
///
/// # Safety
/// Forwarded from buffer-handle lookup and the checked dictionary
/// writer.
pub unsafe fn buffer_set_var(
    buffer: Buffer,
    name: &NvimString,
    value: &Object,
    err: &mut Error,
) -> Object {
    let buffer =
        unsafe { crate::api::private::helpers::find_buffer_by_handle(buffer, err) };
    if buffer.is_null() {
        return Object::Nil;
    }
    unsafe {
        crate::api::private::helpers::dict_set_var(
            (*buffer).b_vars,
            name,
            value,
            false,
            true,
            err,
        )
    }
}

/// Delete a buffer variable and return its previous value
/// (`buffer_del_var`).
///
/// # Safety
/// Forwarded from buffer-handle lookup and the checked dictionary
/// writer.
pub unsafe fn buffer_del_var(
    buffer: Buffer,
    name: &NvimString,
    err: &mut Error,
) -> Object {
    let buffer =
        unsafe { crate::api::private::helpers::find_buffer_by_handle(buffer, err) };
    if buffer.is_null() {
        return Object::Nil;
    }
    unsafe {
        crate::api::private::helpers::dict_set_var(
            (*buffer).b_vars,
            name,
            &Object::Nil,
            true,
            true,
            err,
        )
    }
}

/// Set a window variable and return its previous value
/// (`window_set_var`).
///
/// # Safety
/// Forwarded from window-handle lookup and the checked dictionary
/// writer.
pub unsafe fn window_set_var(
    window: Window,
    name: &NvimString,
    value: &Object,
    err: &mut Error,
) -> Object {
    let window =
        unsafe { crate::api::private::helpers::find_window_by_handle(window, err) };
    if window.is_null() {
        return Object::Nil;
    }
    unsafe {
        crate::api::private::helpers::dict_set_var(
            (*window).w_vars,
            name,
            value,
            false,
            true,
            err,
        )
    }
}

/// Delete a window variable and return its previous value
/// (`window_del_var`).
///
/// # Safety
/// Forwarded from window-handle lookup and the checked dictionary
/// writer.
pub unsafe fn window_del_var(
    window: Window,
    name: &NvimString,
    err: &mut Error,
) -> Object {
    let window =
        unsafe { crate::api::private::helpers::find_window_by_handle(window, err) };
    if window.is_null() {
        return Object::Nil;
    }
    unsafe {
        crate::api::private::helpers::dict_set_var(
            (*window).w_vars,
            name,
            &Object::Nil,
            true,
            true,
            err,
        )
    }
}

/// Get one buffer line through the deprecated API (`buffer_get_line`).
///
/// # Safety
/// Forwarded from [`crate::api::buffer::nvim_buf_get_lines`].
pub unsafe fn buffer_get_line(buffer: Buffer, index: Integer, err: &mut Error) -> NvimString {
    let index = convert_index(index);
    let lines = unsafe {
        crate::api::buffer::nvim_buf_get_lines(0, buffer, index, index + 1, true, err)
    };
    if err.is_set() {
        Vec::new()
    } else {
        lines.into_iter().next().unwrap_or_default()
    }
}

fn convert_slice_range(
    start: Integer,
    end: Integer,
    include_start: Boolean,
    include_end: Boolean,
) -> (Integer, Integer) {
    (
        convert_index(start) + i64::from(!include_start),
        convert_index(end) + i64::from(include_end),
    )
}

/// Get a buffer-line range through the deprecated API
/// (`buffer_get_line_slice`).
///
/// # Safety
/// Forwarded from [`crate::api::buffer::nvim_buf_get_lines`].
pub unsafe fn buffer_get_line_slice(
    buffer: Buffer,
    start: Integer,
    end: Integer,
    include_start: Boolean,
    include_end: Boolean,
    err: &mut Error,
) -> StringArray {
    let (start, end) = convert_slice_range(start, end, include_start, include_end);
    unsafe { crate::api::buffer::nvim_buf_get_lines(0, buffer, start, end, false, err) }
}

/// Get a highlight definition by group ID (`nvim_get_hl_by_id`).
///
/// # Safety
/// `GLOBALS.curwin` must point at a live window. Reads shared
/// highlight-group, namespace/provider, attribute, and font state.
#[must_use]
pub unsafe fn nvim_get_hl_by_id(hl_id: Integer, rgb: Boolean, err: &mut Error) -> Dict {
    if unsafe { crate::highlight_group::syn_get_final_id(hl_id as i32) } == 0 {
        err.r#type = ErrorType::Validation;
        err.msg = Some(format!("Invalid highlight id: {hl_id}"));
        return Vec::new();
    }
    let attr = unsafe { crate::highlight_group::syn_id2attr(hl_id as i32) };
    unsafe { crate::highlight::hl_get_attr_by_id(i64::from(attr), rgb, err) }
}

/// Get a highlight definition by group name
/// (`nvim_get_hl_by_name`).
///
/// # Safety
/// Forwards [`nvim_get_hl_by_id`]'s requirements and may create an
/// `@capture` highlight group through `syn_name2id`.
#[must_use]
pub unsafe fn nvim_get_hl_by_name(name: &NvimString, rgb: Boolean, err: &mut Error) -> Dict {
    let id = unsafe { crate::highlight_group::syn_name2id(name) };
    if id == 0 {
        err.r#type = ErrorType::Validation;
        err.msg = Some(format!(
            "Invalid highlight name: {}",
            std::string::String::from_utf8_lossy(name)
        ));
        return Vec::new();
    }
    unsafe { nvim_get_hl_by_id(i64::from(id), rgb, err) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convert_index_preserves_positive_and_shifts_negative_indexes() {
        assert_eq!(convert_index(0), 0);
        assert_eq!(convert_index(3), 3);
        assert_eq!(convert_index(-1), -2);
        assert_eq!(convert_index(-5), -6);
    }

    #[test]
    fn nvim_buf_get_number_returns_current_buffer_handle() {
        let _lock = crate::globals::global_state_test_lock();
        let buf_ptr = Box::into_raw(Box::new(crate::buffer_defs::BufT {
            handle: 123,
            ..Default::default()
        }));
        let _curbuf =
            unsafe { crate::globals::GlobalFieldGuard::install(|g| &mut g.curbuf, buf_ptr) };
        let mut err = Error::default();
        assert_eq!(unsafe { nvim_buf_get_number(0, &mut err) }, 123);
        assert!(!err.is_set());
        drop(_curbuf);
        unsafe { drop(Box::from_raw(buf_ptr)) };
    }

    #[test]
    fn nvim_buf_get_number_returns_zero_for_an_unknown_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let _lastbuf = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.lastbuf,
                std::ptr::null_mut(),
            )
        };
        let mut err = Error::default();
        assert_eq!(unsafe { nvim_buf_get_number(99, &mut err) }, 0);
        assert_eq!(err.msg.as_deref(), Some("Invalid buffer id: 99"));
    }

    #[test]
    fn nvim_get_option_returns_a_real_global_option_value() {
        let _lock = crate::globals::global_state_test_lock();
        let previous = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ic;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ic = 1;
        let mut err = Error::default();
        let value = unsafe { nvim_get_option(&b"ignorecase".to_vec(), &mut err) };
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ic = previous;
        assert!(matches!(value, Object::Boolean(true)));
        assert!(!err.is_set());
    }

    #[test]
    fn nvim_get_option_rejects_unknown_and_empty_names() {
        let _lock = crate::globals::global_state_test_lock();
        let mut unknown = Error::default();
        assert!(matches!(
            unsafe { nvim_get_option(&b"nero_unknown_option".to_vec(), &mut unknown) },
            Object::Nil
        ));
        assert_eq!(
            unknown.msg.as_deref(),
            Some("Invalid option name: 'nero_unknown_option'")
        );
        let mut empty = Error::default();
        assert!(matches!(
            unsafe { nvim_get_option(&Vec::new(), &mut empty) },
            Object::Nil
        ));
        assert_eq!(empty.msg.as_deref(), Some("Invalid option name: '<empty>'"));
    }

    #[test]
    fn nvim_set_option_writes_a_real_global_option() {
        let _lock = crate::globals::global_state_test_lock();
        let previous = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ic;
        let buf_ptr = Box::into_raw(Box::new(crate::buffer_defs::BufT::default()));
        let synblock_ptr = Box::into_raw(Box::new(crate::buffer_defs::SynblockT::default()));
        let win_ptr = Box::into_raw(Box::new(crate::buffer_defs::WinT {
            w_buffer: buf_ptr,
            w_s: synblock_ptr,
            ..Default::default()
        }));
        let _curbuf =
            unsafe { crate::globals::GlobalFieldGuard::install(|g| &mut g.curbuf, buf_ptr) };
        let _curwin =
            unsafe { crate::globals::GlobalFieldGuard::install(|g| &mut g.curwin, win_ptr) };
        let mut err = Error::default();
        unsafe {
            nvim_set_option(
                &b"ignorecase".to_vec(),
                Object::Boolean(true),
                &mut err,
            )
        };
        let written = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ic;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ic = previous;
        assert_eq!(written, 1);
        assert!(!err.is_set());
        drop(_curwin);
        drop(_curbuf);
        unsafe {
            drop(Box::from_raw(win_ptr));
            drop(Box::from_raw(synblock_ptr));
            drop(Box::from_raw(buf_ptr));
        }
    }

    #[test]
    fn nvim_set_option_rejects_invalid_value_types() {
        let _lock = crate::globals::global_state_test_lock();
        let mut err = Error::default();
        unsafe {
            nvim_set_option(
                &b"ignorecase".to_vec(),
                Object::Float(1.0),
                &mut err,
            )
        };
        assert_eq!(
            err.msg.as_deref(),
            Some("Invalid 'value': expected valid option type, got Float")
        );
    }

    #[test]
    fn vim_set_var_returns_the_previous_global_value() {
        let _lock = crate::globals::global_state_test_lock();
        let key = b"nero_deprecated_set_var";
        let dict = crate::eval::vars::get_globvar_dict();
        assert_eq!(
            unsafe { crate::eval::typval::tv_dict_add_nr(&mut *dict, key, 3) },
            crate::vim_defs::OK
        );
        let mut err = Error::default();
        let old =
            unsafe { vim_set_var(&key.to_vec(), &Object::Integer(7), &mut err) };
        let item = unsafe { crate::eval::typval::tv_dict_find(Some(&mut *dict), key) }
            .expect("global variable");
        let new = unsafe { (*item).di_tv.clone() };
        unsafe { crate::eval::typval::tv_dict_item_remove(&mut *dict, item) };
        assert!(matches!(old, Object::Integer(3)));
        assert!(matches!(
            new.value,
            crate::eval::typval_defs::TypvalValue::Number(7)
        ));
        assert!(!err.is_set());
    }

    #[test]
    fn vim_del_var_returns_the_deleted_global_value() {
        let _lock = crate::globals::global_state_test_lock();
        let key = b"nero_deprecated_del_var";
        let dict = crate::eval::vars::get_globvar_dict();
        assert_eq!(
            unsafe { crate::eval::typval::tv_dict_add_nr(&mut *dict, key, 9) },
            crate::vim_defs::OK
        );
        let mut err = Error::default();
        let old = unsafe { vim_del_var(&key.to_vec(), &mut err) };
        assert!(matches!(old, Object::Integer(9)));
        assert!(unsafe { crate::eval::typval::tv_dict_find(Some(&mut *dict), key) }.is_none());
        assert!(!err.is_set());
    }

    #[test]
    fn buffer_set_var_returns_the_previous_buffer_value() {
        let _lock = crate::globals::global_state_test_lock();
        let dict = crate::eval::typval::tv_dict_alloc();
        assert_eq!(
            unsafe { crate::eval::typval::tv_dict_add_nr(&mut *dict, b"value", 3) },
            crate::vim_defs::OK
        );
        let buf = Box::into_raw(Box::new(crate::buffer_defs::BufT {
            b_vars: dict,
            ..Default::default()
        }));
        let _curbuf =
            unsafe { crate::globals::GlobalFieldGuard::install(|g| &mut g.curbuf, buf) };
        let mut err = Error::default();
        let old =
            unsafe { buffer_set_var(0, &b"value".to_vec(), &Object::Integer(7), &mut err) };
        assert!(matches!(old, Object::Integer(3)));
        assert!(!err.is_set());
        let item = unsafe { crate::eval::typval::tv_dict_find(Some(&mut *dict), b"value") }
            .expect("buffer variable");
        unsafe { crate::eval::typval::tv_dict_item_remove(&mut *dict, item) };
        unsafe { (*buf).b_vars = std::ptr::null_mut() };
        drop(_curbuf);
        unsafe {
            crate::eval::typval::tv_dict_unref(dict);
            drop(Box::from_raw(buf));
        }
    }

    #[test]
    fn buffer_del_var_returns_the_deleted_buffer_value() {
        let _lock = crate::globals::global_state_test_lock();
        let dict = crate::eval::typval::tv_dict_alloc();
        assert_eq!(
            unsafe { crate::eval::typval::tv_dict_add_nr(&mut *dict, b"value", 9) },
            crate::vim_defs::OK
        );
        let buf = Box::into_raw(Box::new(crate::buffer_defs::BufT {
            b_vars: dict,
            ..Default::default()
        }));
        let _curbuf =
            unsafe { crate::globals::GlobalFieldGuard::install(|g| &mut g.curbuf, buf) };
        let mut err = Error::default();
        let old = unsafe { buffer_del_var(0, &b"value".to_vec(), &mut err) };
        assert!(matches!(old, Object::Integer(9)));
        assert!(unsafe { crate::eval::typval::tv_dict_find(Some(&mut *dict), b"value") }.is_none());
        assert!(!err.is_set());
        unsafe { (*buf).b_vars = std::ptr::null_mut() };
        drop(_curbuf);
        unsafe {
            crate::eval::typval::tv_dict_unref(dict);
            drop(Box::from_raw(buf));
        }
    }

    #[test]
    fn window_set_var_returns_the_previous_window_value() {
        let _lock = crate::globals::global_state_test_lock();
        let dict = crate::eval::typval::tv_dict_alloc();
        assert_eq!(
            unsafe { crate::eval::typval::tv_dict_add_nr(&mut *dict, b"value", 3) },
            crate::vim_defs::OK
        );
        let win = Box::into_raw(Box::new(crate::buffer_defs::WinT {
            w_vars: dict,
            ..Default::default()
        }));
        let _curwin =
            unsafe { crate::globals::GlobalFieldGuard::install(|g| &mut g.curwin, win) };
        let mut err = Error::default();
        let old =
            unsafe { window_set_var(0, &b"value".to_vec(), &Object::Integer(7), &mut err) };
        assert!(matches!(old, Object::Integer(3)));
        assert!(!err.is_set());
        let item = unsafe { crate::eval::typval::tv_dict_find(Some(&mut *dict), b"value") }
            .expect("window variable");
        unsafe { crate::eval::typval::tv_dict_item_remove(&mut *dict, item) };
        unsafe { (*win).w_vars = std::ptr::null_mut() };
        drop(_curwin);
        unsafe {
            crate::eval::typval::tv_dict_unref(dict);
            drop(Box::from_raw(win));
        }
    }

    #[test]
    fn window_del_var_returns_the_deleted_window_value() {
        let _lock = crate::globals::global_state_test_lock();
        let dict = crate::eval::typval::tv_dict_alloc();
        assert_eq!(
            unsafe { crate::eval::typval::tv_dict_add_nr(&mut *dict, b"value", 9) },
            crate::vim_defs::OK
        );
        let win = Box::into_raw(Box::new(crate::buffer_defs::WinT {
            w_vars: dict,
            ..Default::default()
        }));
        let _curwin =
            unsafe { crate::globals::GlobalFieldGuard::install(|g| &mut g.curwin, win) };
        let mut err = Error::default();
        let old = unsafe { window_del_var(0, &b"value".to_vec(), &mut err) };
        assert!(matches!(old, Object::Integer(9)));
        assert!(unsafe { crate::eval::typval::tv_dict_find(Some(&mut *dict), b"value") }.is_none());
        assert!(!err.is_set());
        unsafe { (*win).w_vars = std::ptr::null_mut() };
        drop(_curwin);
        unsafe {
            crate::eval::typval::tv_dict_unref(dict);
            drop(Box::from_raw(win));
        }
    }

    #[test]
    fn buffer_get_line_returns_empty_and_an_error_for_an_unknown_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let _lastbuf = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.lastbuf,
                std::ptr::null_mut(),
            )
        };
        let mut err = Error::default();
        let line = unsafe { buffer_get_line(999, 0, &mut err) };
        assert!(line.is_empty());
        assert_eq!(err.msg.as_deref(), Some("Invalid buffer id: 999"));
    }

    #[test]
    fn convert_slice_range_matches_the_legacy_index_rules() {
        assert_eq!(convert_slice_range(0, 3, true, false), (0, 3));
        assert_eq!(convert_slice_range(0, 3, false, true), (1, 4));
        assert_eq!(convert_slice_range(-3, -1, true, true), (-4, -1));
        assert_eq!(convert_slice_range(-3, -1, false, false), (-3, -2));
    }

    #[test]
    fn buffer_get_line_slice_returns_empty_and_an_error_for_an_unknown_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let _lastbuf = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.lastbuf,
                std::ptr::null_mut(),
            )
        };
        let mut err = Error::default();
        let lines = unsafe { buffer_get_line_slice(999, 0, -1, true, false, &mut err) };
        assert!(lines.is_empty());
        assert_eq!(err.msg.as_deref(), Some("Invalid buffer id: 999"));
    }

    struct HighlightApiGuard {
        groups: Vec<crate::highlight_group::HlGroup>,
        names: crate::map::Map<Vec<u8>, i32>,
        attrs: crate::map::Set<crate::highlight_defs::HlEntry>,
        curwin: *mut crate::buffer_defs::WinT,
        ns_active: i32,
    }

    impl HighlightApiGuard {
        fn empty(curwin: *mut crate::buffer_defs::WinT) -> Self {
            let table = unsafe { crate::highlight_group::HL_TABLE.get_mut() };
            let groups = std::mem::take(&mut table.items);
            let names = std::mem::replace(
                unsafe { crate::highlight_group::HIGHLIGHT_UNAMES.get_mut() },
                crate::map::Map::new(),
            );
            let attrs = std::mem::replace(
                unsafe { crate::highlight::ATTR_ENTRIES.get_mut() },
                crate::map::Set::new(),
            );
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            let old_curwin = globals.curwin;
            globals.curwin = curwin;
            let ns_active = unsafe { *crate::highlight::NS_HL_ACTIVE.get_mut() };
            unsafe { *crate::highlight::NS_HL_ACTIVE.get_mut() = 0 };
            Self {
                groups,
                names,
                attrs,
                curwin: old_curwin,
                ns_active,
            }
        }
    }

    impl Drop for HighlightApiGuard {
        fn drop(&mut self) {
            unsafe { crate::highlight_group::HL_TABLE.get_mut() }.items =
                std::mem::take(&mut self.groups);
            *unsafe { crate::highlight_group::HIGHLIGHT_UNAMES.get_mut() } =
                std::mem::replace(&mut self.names, crate::map::Map::new());
            *unsafe { crate::highlight::ATTR_ENTRIES.get_mut() } =
                std::mem::replace(&mut self.attrs, crate::map::Set::new());
            unsafe { crate::globals::GLOBALS.get_mut() }.curwin = self.curwin;
            unsafe { *crate::highlight::NS_HL_ACTIVE.get_mut() = self.ns_active };
        }
    }

    fn dict_value<'a>(
        dict: &'a Dict,
        key: &[u8],
    ) -> Option<&'a crate::api::private::defs::Object> {
        dict.iter().find(|item| item.key == key).map(|item| &item.value)
    }

    #[test]
    fn nvim_get_hl_by_id_returns_the_resolved_definition() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = crate::buffer_defs::WinT::default();
        let win_ptr = std::ptr::addr_of_mut!(win);
        let _guard = HighlightApiGuard::empty(win_ptr);
        let id = unsafe { crate::highlight_group::syn_check_group(b"Comment") };
        let attrs = crate::highlight_defs::HlAttrs {
            rgb_fg_color: 0x12_34_56,
            ..Default::default()
        };
        let attr = unsafe { crate::highlight::hl_get_term_attr(&attrs) };
        unsafe { crate::highlight_group::HL_TABLE.get_mut() }.items[(id - 1) as usize].sg_attr =
            attr;
        let mut err = Error::default();

        let result = unsafe { nvim_get_hl_by_id(i64::from(id), true, &mut err) };

        assert!(!err.is_set());
        assert!(matches!(
            dict_value(&result, b"foreground"),
            Some(crate::api::private::defs::Object::Integer(0x12_34_56))
        ));
    }

    #[test]
    fn nvim_get_hl_by_name_is_case_insensitive() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = crate::buffer_defs::WinT::default();
        let win_ptr = std::ptr::addr_of_mut!(win);
        let _guard = HighlightApiGuard::empty(win_ptr);
        let id = unsafe { crate::highlight_group::syn_check_group(b"Comment") };
        let attr = unsafe {
            crate::highlight::hl_get_term_attr(&crate::highlight_defs::HlAttrs {
                cterm_fg_color: 3,
                ..Default::default()
            })
        };
        unsafe { crate::highlight_group::HL_TABLE.get_mut() }.items[(id - 1) as usize].sg_attr =
            attr;
        let mut err = Error::default();

        let result = unsafe { nvim_get_hl_by_name(&b"comment".to_vec(), false, &mut err) };

        assert!(!err.is_set());
        assert!(matches!(
            dict_value(&result, b"foreground"),
            Some(crate::api::private::defs::Object::Integer(2))
        ));
    }

    #[test]
    fn deprecated_highlight_lookups_report_validation_errors() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = crate::buffer_defs::WinT::default();
        let win_ptr = std::ptr::addr_of_mut!(win);
        let _guard = HighlightApiGuard::empty(win_ptr);
        let mut err = Error::default();

        assert!(unsafe { nvim_get_hl_by_id(99, true, &mut err) }.is_empty());
        assert_eq!(err.r#type, ErrorType::Validation);
        assert_eq!(err.msg.as_deref(), Some("Invalid highlight id: 99"));

        err = Error::default();
        assert!(unsafe { nvim_get_hl_by_name(&b"Missing".to_vec(), true, &mut err) }.is_empty());
        assert_eq!(err.msg.as_deref(), Some("Invalid highlight name: Missing"));
    }
}
