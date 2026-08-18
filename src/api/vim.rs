//! Translated from `src/nvim/api/vim.c` (tractable subset only - most
//! of this huge file needs `Array`/`Dict`/`Object`-conversion
//! machinery, the msgpack-rpc dispatch layer, command execution, and
//! the Lua host, none of which are wired into this API layer yet).
//!
//! Translated: [`nvim_list_bufs`], [`nvim_list_wins`], [`nvim_list_tabpages`],
//! [`nvim_get_current_win`]/
//! [`nvim_get_current_buf`]/[`nvim_get_current_tabpage`], [`nvim_get_current_line`]
//! (harvested ahead of the rest of this
//! file, matching the established "one tractable function ahead of a
//! huge file" precedent used elsewhere in this crate, e.g.
//! `ex_docmd.rs`; `nvim_get_current_win` is also `api/tabpage.c`'s own
//! real dependency - `nvim_tabpage_get_win` calls it directly when the
//! tabpage in question is the current one), and [`nvim_strwidth`] (via
//! the already-existing `mbyte.rs::mb_string2cells`).

use crate::api::private::defs::{
    Array, Boolean, Buffer, Dict, Error, ErrorType, Integer, NvimString, Object, Tabpage,
    Window,
};

/// Return a deep owned copy of `obj` (`nvim__id`).
#[must_use]
#[allow(non_snake_case)]
pub fn nvim__id(obj: &Object) -> Object {
    obj.clone()
}

/// Return a deep owned copy of `arr` (`nvim__id_array`).
#[must_use]
#[allow(non_snake_case)]
pub fn nvim__id_array(arr: &Array) -> Array {
    arr.clone()
}

/// Return a deep owned copy of `dict` (`nvim__id_dict`).
#[must_use]
#[allow(non_snake_case)]
pub fn nvim__id_dict(dict: &Dict) -> Dict {
    dict.clone()
}

/// Return `value` unchanged (`nvim__id_float`).
#[must_use]
#[allow(non_snake_case)]
pub fn nvim__id_float(value: f64) -> f64 {
    value
}

/// Return the current editor mode and input-blocking state
/// (`nvim_get_mode`).
///
/// # Safety
/// Forwarded from [`crate::state::get_mode`] and
/// [`crate::os::input::input_blocking`].
#[must_use]
pub unsafe fn nvim_get_mode() -> Dict {
    vec![
        crate::api::private::defs::KeyValuePair {
            key: b"mode".to_vec(),
            value: Object::String(unsafe { crate::state::get_mode() }),
        },
        crate::api::private::defs::KeyValuePair {
            key: b"blocking".to_vec(),
            value: Object::Boolean(unsafe { crate::os::input::input_blocking() }),
        },
    ]
}

/// Resolve a named or hexadecimal RGB color
/// (`nvim_get_color_by_name`).
///
/// # Safety
/// Forwarded from [`crate::highlight_group::name_to_color`].
#[must_use]
pub unsafe fn nvim_get_color_by_name(name: &NvimString) -> Integer {
    i64::from(unsafe { crate::highlight_group::name_to_color(name) }.0)
}

/// Define a highlight group in a namespace (`nvim_set_hl`).
///
/// # Safety
/// Mutates shared highlight-group, namespace, provider, attribute,
/// font, redraw, and script-context state.
pub unsafe fn nvim_set_hl(
    ns_id: Integer,
    name: &NvimString,
    value: &crate::highlight::HighlightDict,
    err: &mut Error,
) {
    let hl_id = unsafe { crate::highlight_group::syn_check_group(name) };
    if hl_id == 0 {
        return;
    }

    if value.url.is_some() {
        err.r#type = ErrorType::Validation;
        err.msg = Some("Invalid key: 'url'".to_string());
        return;
    }
    let mut link_id = -1;
    let mut base_attrs = crate::highlight_defs::HlAttrs::default();
    let base = if value.update.unwrap_or(false)
        && unsafe {
            crate::highlight::hl_ns_get_attrs(ns_id as i32, hl_id, None, &mut base_attrs)
        }
    {
        Some(&base_attrs)
    } else {
        None
    };
    let attrs =
        unsafe { crate::highlight::dict2hlattrs(value, true, Some(&mut link_id), base, err) };
    if !err.is_set() {
        unsafe {
            crate::highlight::ns_hl_def(
                ns_id as i32,
                hl_id,
                attrs,
                link_id,
                Some(value),
            )
        };
    }
}

/// Selection controls for [`nvim_get_hl`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GetHighlightOpts {
    pub create: Option<Boolean>,
    pub id: Option<Integer>,
    pub link: Option<Boolean>,
    pub name: Option<NvimString>,
}

/// Return one or all highlight definitions from a namespace
/// (`nvim_get_hl`).
///
/// # Safety
/// Reads and may extend shared highlight-group state, and reads
/// namespace/provider/attribute/font state.
#[must_use]
pub unsafe fn nvim_get_hl(
    ns_id: Integer,
    opts: &GetHighlightOpts,
    err: &mut Error,
) -> Dict {
    let link = opts.link.unwrap_or(true);
    let mut id = -1;
    if let Some(name) = &opts.name {
        id = if opts.create.unwrap_or(true) {
            unsafe { crate::highlight_group::syn_check_group(name) }
        } else {
            unsafe { crate::highlight_group::syn_name2id_len(name) }
        };
        if id == 0 && !opts.create.unwrap_or(true) {
            return Vec::new();
        }
    } else if let Some(requested) = opts.id {
        id = requested as i32;
    }
    let len = unsafe { crate::highlight_group::HL_TABLE.get_mut() }.items.len() as i32;
    if id != -1 {
        if !(1..=len).contains(&id) {
            err.r#type = ErrorType::Validation;
            err.msg = Some("Highlight id out of bounds".to_string());
            return Vec::new();
        }
        let resolved = if link {
            id
        } else {
            unsafe { crate::highlight_group::syn_get_final_id(id) }
        };
        return unsafe { crate::highlight_group::hlgroup2dict(ns_id as i32, resolved) }
            .unwrap_or_default();
    }

    let mut result = Vec::with_capacity(len as usize);
    for id in 1..=len {
        let Some(attrs) = (unsafe {
            crate::highlight_group::hlgroup2dict(ns_id as i32, id)
        }) else {
            continue;
        };
        let name_id = if link {
            id
        } else {
            unsafe { crate::highlight_group::syn_get_final_id(id) }
        };
        let name = unsafe { crate::highlight_group::HL_TABLE.get_mut() }.items
            [(name_id - 1) as usize]
            .sg_name
            .clone();
        result.push(crate::api::private::defs::KeyValuePair {
            key: name,
            value: Object::Dict(attrs),
        });
    }
    result
}

/// Get or create a highlight-group ID by name
/// (`nvim_get_hl_id_by_name`).
///
/// # Safety
/// Mutates the shared highlight-group registry when `name` is new.
#[must_use]
pub unsafe fn nvim_get_hl_id_by_name(name: &NvimString) -> Integer {
    i64::from(unsafe { crate::highlight_group::syn_check_group(name) })
}

/// Return an uppercase/file mark tuple (`nvim_get_mark`).
///
/// # Safety
/// Reads the shared global-mark and buffer registries.
#[must_use]
pub unsafe fn nvim_get_mark(name: &NvimString, err: &mut Error) -> Array {
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
    let mark_name = name[0];
    if !mark_name.is_ascii_uppercase() && !mark_name.is_ascii_digit() {
        err.r#type = ErrorType::Validation;
        err.msg = Some(format!(
            "Invalid mark name (must be file/uppercase): '{}'",
            String::from_utf8_lossy(name)
        ));
        return Vec::new();
    }
    let mark = unsafe { crate::mark::mark_get_global(false, i32::from(mark_name)) };
    let position = unsafe { (*mark).fmark.mark };
    let (buffer, filename) = if unsafe { (*mark).fmark.fnum } != 0 {
        let buffer = unsafe { (*mark).fmark.fnum };
        (
            buffer,
            unsafe { crate::buffer::buflist_nr2name(buffer, true, true) },
        )
    } else {
        (0, unsafe { (*mark).fname.clone() })
    };
    let (row, col, buffer, filename) = if filename.is_none() || position.lnum <= 0 {
        (0, 0, 0, Vec::new())
    } else {
        (
            i64::from(position.lnum),
            i64::from(position.col),
            i64::from(buffer),
            filename.unwrap_or_default(),
        )
    };
    vec![
        Object::Integer(row),
        Object::Integer(col),
        Object::Integer(buffer),
        Object::String(filename),
    ]
}

/// Return the complete named RGB color map (`nvim_get_color_map`).
#[must_use]
pub fn nvim_get_color_map() -> Dict {
    crate::highlight_group::COLOR_NAME_TABLE
        .iter()
        .map(|(name, value)| crate::api::private::defs::KeyValuePair {
            key: name.to_vec(),
            value: Object::Integer(i64::from(*value)),
        })
        .collect()
}

/// List every current buffer, including unlisted and unloaded buffers
/// (`nvim_list_bufs`).
///
/// # Safety
/// `GLOBALS.firstbuf` and each `b_next` link must form a live buffer
/// list.
#[must_use]
pub unsafe fn nvim_list_bufs() -> Array {
    let mut result = Vec::new();
    let mut buf = unsafe { crate::globals::GLOBALS.get_mut() }.firstbuf;
    while !buf.is_null() {
        result.push(Object::Buffer(unsafe { (*buf).handle }));
        buf = unsafe { (*buf).b_next };
    }
    result
}

/// List every current window in every tabpage (`nvim_list_wins`).
///
/// # Safety
/// The tabpage list and every tabpage's window list must consist of
/// live pointers.
#[must_use]
pub unsafe fn nvim_list_wins() -> Array {
    let globals = unsafe { crate::globals::GLOBALS.get_mut() };
    let current_tab = globals.curtab;
    let current_firstwin = globals.firstwin;
    let mut tab = globals.first_tabpage;
    let mut result = Vec::new();
    while !tab.is_null() {
        let mut win = if std::ptr::eq(tab, current_tab) {
            current_firstwin
        } else {
            unsafe { (*tab).tp_firstwin }
        };
        while !win.is_null() {
            result.push(Object::Window(unsafe { (*win).handle }));
            win = unsafe { (*win).w_next };
        }
        tab = unsafe { (*tab).tp_next };
    }
    result
}

/// List every current tabpage (`nvim_list_tabpages`).
///
/// # Safety
/// `GLOBALS.first_tabpage` and each `tp_next` link must form a live
/// tabpage list.
#[must_use]
pub unsafe fn nvim_list_tabpages() -> Array {
    let mut result = Vec::new();
    let mut tab = unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage;
    while !tab.is_null() {
        result.push(Object::Tabpage(unsafe { (*tab).handle }));
        tab = unsafe { (*tab).tp_next };
    }
    result
}

/// Get a global variable (`nvim_get_var`).
///
/// # Safety
/// Reads the shared global-variable dictionary and recursively
/// converts the selected value.
pub unsafe fn nvim_get_var(name: &NvimString, err: &mut Error) -> Object {
    let dict = crate::eval::vars::get_globvar_dict();
    if unsafe { crate::eval::typval::tv_dict_find(Some(&mut *dict), name) }.is_none() {
        let found = crate::runtime::script_autoload(name, false)
            && !crate::ex_eval::aborting();
        if !found {
            err.r#type = ErrorType::Validation;
            err.msg = Some(format!("Key not found: {}", String::from_utf8_lossy(name)));
            return Object::Nil;
        }
    }
    unsafe {
        crate::api::private::helpers::dict_get_value(
            dict,
            name,
            err,
        )
    }
}

/// Set a global variable (`nvim_set_var`).
///
/// # Safety
/// Mutates the shared global-variable dictionary and may allocate
/// nested eval containers.
pub unsafe fn nvim_set_var(name: &NvimString, value: &Object, err: &mut Error) {
    let _ = unsafe {
        crate::api::private::helpers::dict_set_var(
            crate::eval::vars::get_globvar_dict(),
            name,
            value,
            false,
            false,
            err,
        )
    };
}

/// Delete a global variable (`nvim_del_var`).
///
/// # Safety
/// Mutates the shared global-variable dictionary and may release
/// nested eval containers.
pub unsafe fn nvim_del_var(name: &NvimString, err: &mut Error) {
    let _ = unsafe {
        crate::api::private::helpers::dict_set_var(
            crate::eval::vars::get_globvar_dict(),
            name,
            &Object::Nil,
            true,
            false,
            err,
        )
    };
}

/// Get a special `v:` variable (`nvim_get_vvar`).
///
/// # Safety
/// Reads the shared special-variable dictionary and recursively
/// converts the selected value.
pub unsafe fn nvim_get_vvar(name: &NvimString, err: &mut Error) -> Object {
    unsafe {
        crate::api::private::helpers::dict_get_value(
            crate::eval::vars::get_vimvar_dict(),
            name,
            err,
        )
    }
}

/// Set a writable special `v:` variable (`nvim_set_vvar`).
///
/// # Safety
/// Mutates the shared special-variable dictionary and may update
/// search/redraw state for special variables with assignment hooks.
pub unsafe fn nvim_set_vvar(name: &NvimString, value: &Object, err: &mut Error) {
    let _ = unsafe {
        crate::api::private::helpers::dict_set_var(
            crate::eval::vars::get_vimvar_dict(),
            name,
            value,
            false,
            false,
            err,
        )
    };
}

/// Get the current window's handle (`nvim_get_current_win`).
///
/// # Safety
/// `GLOBALS.curwin` must be a valid, non-null pointer to a live
/// `WinT`.
#[must_use]
pub unsafe fn nvim_get_current_win() -> Window {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { &*crate::globals::GLOBALS.get_mut().curwin }.handle
}

/// Get the current buffer's handle (`nvim_get_current_buf`).
///
/// # Safety
/// `GLOBALS.curbuf` must be a valid, non-null pointer to a live
/// `BufT`.
#[must_use]
pub unsafe fn nvim_get_current_buf() -> Buffer {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { &*crate::globals::GLOBALS.get_mut().curbuf }.handle
}

/// Get the current tab page's handle (`nvim_get_current_tabpage`).
///
/// # Safety
/// `GLOBALS.curtab` must be a valid, non-null pointer to a live
/// `TabpageT`.
#[must_use]
pub unsafe fn nvim_get_current_tabpage() -> Tabpage {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { &*crate::globals::GLOBALS.get_mut().curtab }.handle
}

/// Get the current cursor line (`nvim_get_current_line`).
///
/// # Safety
/// `GLOBALS.curbuf` and `GLOBALS.curwin` must point to live objects.
pub unsafe fn nvim_get_current_line(err: &mut Error) -> NvimString {
    let globals = unsafe { crate::globals::GLOBALS.get_mut() };
    unsafe {
        crate::api::deprecated::buffer_get_line(
            (*globals.curbuf).handle,
            i64::from((*globals.curwin).w_cursor.lnum) - 1,
            err,
        )
    }
}

/// Whether byte length `len` passes `nvim_strwidth`'s own length
/// guard (`text.size <= INT_MAX` in the original). Factored out from
/// [`nvim_strwidth`] itself so its exact boundary can be tested
/// directly - exercising the real "too long" branch end-to-end would
/// need constructing a genuine ~2 GiB `Vec<u8>`, impractical for a
/// test that may run dozens of times per flakiness check.
#[must_use]
fn text_length_ok(len: usize) -> bool {
    len <= i32::MAX as usize
}

/// Calculate the number of display cells `text` occupies
/// (`nvim_strwidth`), or `0` with a real, structured `Error` when
/// `text` is longer than `i32::MAX` bytes (matching the original's
/// own `VALIDATE_S`/`api_err_invalid` message format exactly:
/// `"Invalid text length: '(too long)'"`).
///
/// # Safety
/// Forwarded from [`crate::mbyte::mb_string2cells`]'s own safety doc.
pub unsafe fn nvim_strwidth(text: &NvimString, err: &mut Error) -> Integer {
    if !text_length_ok(text.len()) {
        err.r#type = ErrorType::Validation;
        err.msg = Some("Invalid text length: '(too long)'".to_string());
        return 0;
    }

    // Matches the original's own unchecked `(Integer)mb_string2cells(...)`
    // cast - `mb_string2cells`'s own result can be at most roughly
    // `2 * text.len()` (every character occupies at most 2 display
    // cells), which is always well within `i64`'s range given the
    // `i32::MAX` bound on `text.len()` just above.
    // SAFETY: forwarded from this function's own safety doc.
    (unsafe { crate::mbyte::mb_string2cells(text) }) as i64
}

/// Get the active highlight namespace (`nvim_get_hl_ns`).
///
/// `winid == None` selects the global namespace; a window ID returns
/// that window's local namespace.
///
/// # Safety
/// Forwarded from `find_window_by_handle` when `winid` is present, and
/// otherwise reads shared highlight state.
pub unsafe fn nvim_get_hl_ns(winid: Option<Window>, err: &mut Error) -> Integer {
    if let Some(winid) = winid {
        let win = unsafe { crate::api::private::helpers::find_window_by_handle(winid, err) };
        if win.is_null() {
            0
        } else {
            i64::from(unsafe { (*win).w_ns_hl })
        }
    } else {
        i64::from(unsafe { *crate::highlight::NS_HL_GLOBAL.get_mut() })
    }
}

/// Set the active global highlight namespace (`nvim_set_hl_ns`).
///
/// # Safety
/// Mutates shared highlight namespace/provider/group state and
/// schedules a redraw for every live window.
pub unsafe fn nvim_set_hl_ns(ns_id: Integer, err: &mut Error) {
    if ns_id < 0 {
        err.r#type = ErrorType::Validation;
        err.msg = Some(format!("Invalid 'namespace': {ns_id}"));
        return;
    }
    unsafe { *crate::highlight::NS_HL_GLOBAL.get_mut() = ns_id as i32 };
    let _ = unsafe { crate::highlight::hl_check_ns() };
    unsafe { crate::drawscreen::redraw_all_later(crate::drawscreen::UPD_NOT_VALID) };
}

/// Set the fast-callback highlight namespace
/// (`nvim_set_hl_ns_fast`).
///
/// Unlike [`nvim_set_hl_ns`], the original deliberately performs no
/// nonnegative validation.
///
/// # Safety
/// Mutates shared highlight namespace/provider/group state.
pub unsafe fn nvim_set_hl_ns_fast(ns_id: Integer) {
    unsafe { *crate::highlight::NS_HL_FAST.get_mut() = ns_id as i32 };
    let _ = unsafe { crate::highlight::hl_check_ns() };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer_defs::{BufT, TabpageT, WinT};

    #[test]
    fn nvim_list_bufs_returns_every_linked_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let mut second = BufT {
            handle: 72,
            ..Default::default()
        };
        let second_ptr = std::ptr::addr_of_mut!(second);
        let mut first = BufT {
            handle: 71,
            b_next: second_ptr,
            ..Default::default()
        };
        let first_ptr = std::ptr::addr_of_mut!(first);
        let _firstbuf = unsafe {
            crate::globals::GlobalFieldGuard::install(|g| &mut g.firstbuf, first_ptr)
        };

        let buffers = unsafe { nvim_list_bufs() };

        assert!(matches!(
            buffers.as_slice(),
            [Object::Buffer(71), Object::Buffer(72)]
        ));
    }

    #[test]
    fn nvim_list_bufs_returns_empty_for_an_empty_buffer_list() {
        let _lock = crate::globals::global_state_test_lock();
        let _firstbuf = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.firstbuf,
                std::ptr::null_mut(),
            )
        };
        assert!(unsafe { nvim_list_bufs() }.is_empty());
    }

    #[test]
    fn nvim_list_wins_returns_windows_from_every_tabpage() {
        let _lock = crate::globals::global_state_test_lock();
        let mut other_win = WinT {
            handle: 83,
            ..Default::default()
        };
        let other_win_ptr = std::ptr::addr_of_mut!(other_win);
        let mut current_second = WinT {
            handle: 82,
            ..Default::default()
        };
        let current_second_ptr = std::ptr::addr_of_mut!(current_second);
        let mut current_first = WinT {
            handle: 81,
            w_next: current_second_ptr,
            ..Default::default()
        };
        let current_first_ptr = std::ptr::addr_of_mut!(current_first);
        let mut other_tab = TabpageT {
            handle: 92,
            tp_firstwin: other_win_ptr,
            ..Default::default()
        };
        let other_tab_ptr = std::ptr::addr_of_mut!(other_tab);
        let mut current_tab = TabpageT {
            handle: 91,
            tp_next: other_tab_ptr,
            ..Default::default()
        };
        let current_tab_ptr = std::ptr::addr_of_mut!(current_tab);
        let _first_tabpage = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.first_tabpage,
                current_tab_ptr,
            )
        };
        let _curtab = unsafe {
            crate::globals::GlobalFieldGuard::install(|g| &mut g.curtab, current_tab_ptr)
        };
        let _firstwin = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.firstwin,
                current_first_ptr,
            )
        };

        let windows = unsafe { nvim_list_wins() };

        assert!(matches!(
            windows.as_slice(),
            [Object::Window(81), Object::Window(82), Object::Window(83)]
        ));
    }

    #[test]
    fn nvim_list_wins_returns_empty_without_tabpages() {
        let _lock = crate::globals::global_state_test_lock();
        let _first_tabpage = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.first_tabpage,
                std::ptr::null_mut(),
            )
        };
        assert!(unsafe { nvim_list_wins() }.is_empty());
    }

    #[test]
    fn nvim_list_tabpages_returns_every_linked_tabpage() {
        let _lock = crate::globals::global_state_test_lock();
        let mut second = TabpageT {
            handle: 102,
            ..Default::default()
        };
        let second_ptr = std::ptr::addr_of_mut!(second);
        let mut first = TabpageT {
            handle: 101,
            tp_next: second_ptr,
            ..Default::default()
        };
        let first_ptr = std::ptr::addr_of_mut!(first);
        let _first_tabpage = unsafe {
            crate::globals::GlobalFieldGuard::install(|g| &mut g.first_tabpage, first_ptr)
        };

        let tabpages = unsafe { nvim_list_tabpages() };

        assert!(matches!(
            tabpages.as_slice(),
            [Object::Tabpage(101), Object::Tabpage(102)]
        ));
    }

    #[test]
    fn nvim_list_tabpages_returns_empty_for_an_empty_list() {
        let _lock = crate::globals::global_state_test_lock();
        let _first_tabpage = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.first_tabpage,
                std::ptr::null_mut(),
            )
        };
        assert!(unsafe { nvim_list_tabpages() }.is_empty());
    }

    #[test]
    fn nvim_get_var_returns_a_real_global_variable() {
        let _lock = crate::globals::global_state_test_lock();
        let dict = crate::eval::vars::get_globvar_dict();
        let key = b"nero_api_global_answer";
        assert_eq!(
            unsafe { crate::eval::typval::tv_dict_add_nr(&mut *dict, key, 42) },
            crate::vim_defs::OK
        );
        let mut err = Error::default();
        let value = unsafe { nvim_get_var(&key.to_vec(), &mut err) };
        let item = unsafe { crate::eval::typval::tv_dict_find(Some(&mut *dict), key) }
            .expect("global variable");
        unsafe { crate::eval::typval::tv_dict_item_remove(&mut *dict, item) };
        assert!(matches!(value, Object::Integer(42)));
        assert!(!err.is_set());
    }

    #[test]
    fn nvim_set_var_inserts_and_updates_a_global_variable() {
        let _lock = crate::globals::global_state_test_lock();
        let key = b"nero_api_set_global";
        let mut err = Error::default();
        unsafe { nvim_set_var(&key.to_vec(), &Object::Integer(3), &mut err) };
        unsafe { nvim_set_var(&key.to_vec(), &Object::Integer(7), &mut err) };
        let dict = crate::eval::vars::get_globvar_dict();
        let item = unsafe { crate::eval::typval::tv_dict_find(Some(&mut *dict), key) }
            .expect("global variable");
        let stored = unsafe { (*item).di_tv.clone() };
        unsafe { crate::eval::typval::tv_dict_item_remove(&mut *dict, item) };
        assert!(matches!(stored.value, crate::eval::typval_defs::TypvalValue::Number(7)));
        assert!(!err.is_set());
    }

    #[test]
    fn nvim_del_var_removes_a_global_variable() {
        let _lock = crate::globals::global_state_test_lock();
        let key = b"nero_api_del_global";
        let mut err = Error::default();
        unsafe { nvim_set_var(&key.to_vec(), &Object::Integer(3), &mut err) };
        unsafe { nvim_del_var(&key.to_vec(), &mut err) };
        let dict = crate::eval::vars::get_globvar_dict();
        assert!(unsafe { crate::eval::typval::tv_dict_find(Some(&mut *dict), key) }.is_none());
        assert!(!err.is_set());
    }

    #[test]
    fn nvim_get_vvar_reads_the_special_variable_dictionary() {
        let _lock = crate::globals::global_state_test_lock();
        let mut false_err = Error::default();
        assert!(matches!(
            unsafe { nvim_get_vvar(&b"false".to_vec(), &mut false_err) },
            Object::Boolean(false)
        ));
        assert!(!false_err.is_set());

        let mut missing_err = Error::default();
        assert!(matches!(
            unsafe { nvim_get_vvar(&b"nero_missing_vvar".to_vec(), &mut missing_err) },
            Object::Nil
        ));
        assert_eq!(
            missing_err.msg.as_deref(),
            Some("Key not found: nero_missing_vvar")
        );
    }

    #[test]
    fn nvim_get_current_line_returns_the_cursor_line() {
        let _lock = crate::globals::global_state_test_lock();
        let buf_ptr = Box::into_raw(Box::new(BufT::default()));
        assert_eq!(unsafe { crate::memline::ml_open(&mut *buf_ptr) }, crate::vim_defs::OK);
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(&mut *buf_ptr, 1, b"first\0") },
            crate::vim_defs::OK
        );
        assert_eq!(
            unsafe { crate::memline::ml_append_buf(&mut *buf_ptr, 1, b"second\0", 7, false) },
            crate::vim_defs::OK
        );
        let win_ptr = Box::into_raw(Box::new(WinT {
            w_buffer: buf_ptr,
            w_cursor: crate::pos_defs::PosT {
                lnum: 2,
                col: 0,
                coladd: 0,
            },
            ..Default::default()
        }));
        let _curbuf =
            unsafe { crate::globals::GlobalFieldGuard::install(|g| &mut g.curbuf, buf_ptr) };
        let _curwin =
            unsafe { crate::globals::GlobalFieldGuard::install(|g| &mut g.curwin, win_ptr) };
        let _lastbuf =
            unsafe { crate::globals::GlobalFieldGuard::install(|g| &mut g.lastbuf, buf_ptr) };
        let mut err = Error::default();

        let line = unsafe { nvim_get_current_line(&mut err) };

        assert_eq!(line, b"second");
        assert!(!err.is_set());
        unsafe {
            let mfp = (*buf_ptr).b_ml.ml_mfp;
            (*buf_ptr).b_ml.ml_mfp = std::ptr::null_mut();
            crate::memfile::mf_close(*Box::from_raw(mfp), false);
            drop(Box::from_raw(win_ptr));
            drop(Box::from_raw(buf_ptr));
        }

    }

    #[test]
    fn nvim_id_returns_an_independent_nested_object_copy() {
        let input = Object::Array(vec![Object::String(b"one".to_vec())]);
        let mut output = nvim__id(&input);
        if let Object::Array(items) = &mut output
            && let Object::String(text) = &mut items[0]
        {
            text[0] = b'd';
        }

        assert!(matches!(
            input,
            Object::Array(ref items) if matches!(&items[0], Object::String(text) if text == b"one")
        ));
        assert!(matches!(
            output,
            Object::Array(ref items) if matches!(&items[0], Object::String(text) if text == b"dne")
        ));
    }

    #[test]
    fn nvim_id_array_returns_an_independent_copy() {
        let input = vec![Object::String(b"one".to_vec())];
        let mut output = nvim__id_array(&input);
        if let Object::String(text) = &mut output[0] {
            text[0] = b'd';
        }
        assert!(matches!(&input[0], Object::String(text) if text == b"one"));
        assert!(matches!(&output[0], Object::String(text) if text == b"dne"));
    }

    #[test]
    fn nvim_id_dict_returns_an_independent_copy() {
        let input = vec![crate::api::private::defs::KeyValuePair {
            key: b"name".to_vec(),
            value: Object::String(b"one".to_vec()),
        }];
        let mut output = nvim__id_dict(&input);
        output[0].key[0] = b'g';
        if let Object::String(text) = &mut output[0].value {
            text[0] = b'd';
        }
        assert_eq!(input[0].key, b"name");
        assert_eq!(output[0].key, b"game");
        assert!(matches!(&input[0].value, Object::String(text) if text == b"one"));
        assert!(matches!(&output[0].value, Object::String(text) if text == b"dne"));
    }

    #[test]
    fn nvim_id_float_returns_its_argument() {
        assert_eq!(nvim__id_float(3.25), 3.25);
        assert!(nvim__id_float(f64::NAN).is_nan());
        assert_eq!(nvim__id_float(f64::INFINITY), f64::INFINITY);
    }

    #[test]
    fn nvim_get_mode_returns_normal_and_unblocked_by_default() {
        let _lock = crate::globals::global_state_test_lock();
        let buf_ptr = Box::into_raw(Box::new(BufT::default()));
        let _curbuf =
            unsafe { crate::globals::GlobalFieldGuard::install(|g| &mut g.curbuf, buf_ptr) };
        let _state = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.State,
                crate::state_defs::mode::NORMAL as i32,
            )
        };

        let mode = unsafe { nvim_get_mode() };

        assert!(mode.iter().any(|pair| {
            pair.key == b"mode" && matches!(&pair.value, Object::String(value) if value == b"n")
        }));
        assert!(mode.iter().any(|pair| {
            pair.key == b"blocking"
                && matches!(pair.value, Object::Boolean(false))
        }));
        drop(_state);
        drop(_curbuf);
        unsafe { drop(Box::from_raw(buf_ptr)) };
    }

    #[test]
    fn nvim_get_color_by_name_resolves_named_hex_and_invalid_colors() {
        assert_eq!(
            unsafe { nvim_get_color_by_name(&b"RebeccaPurple".to_vec()) },
            0x663399
        );
        assert_eq!(
            unsafe { nvim_get_color_by_name(&b"#12abEF".to_vec()) },
            0x12abef
        );
        assert_eq!(
            unsafe { nvim_get_color_by_name(&b"not-a-color".to_vec()) },
            -1
        );
    }

    #[test]
    fn nvim_get_color_map_returns_every_named_color() {
        let colors = nvim_get_color_map();
        assert_eq!(colors.len(), 707);
        assert!(colors.iter().any(|pair| {
            pair.key == b"RebeccaPurple"
                && matches!(pair.value, Object::Integer(0x663399))
        }));
        assert!(colors.iter().any(|pair| {
            pair.key == b"YellowGreen"
                && matches!(pair.value, Object::Integer(0x9acd32))
        }));
    }

    #[test]
    fn nvim_set_hl_defines_a_namespaced_highlight() {
        let _lock = crate::globals::global_state_test_lock();
        let namespace = 4001;
        let name = b"NeroApiNamespacedHighlight".to_vec();
        let mut err = Error::default();
        unsafe {
            nvim_set_hl(
                namespace,
                &name,
                &crate::highlight::HighlightDict {
                    bold: Some(true),
                    fg: Some(Object::Integer(0x123456)),
                    ..Default::default()
                },
                &mut err,
            )
        };
        assert!(!err.is_set());
        let hl_id = unsafe { crate::highlight_group::syn_name2id(&name) };
        let item = *unsafe { crate::highlight::NS_HLS.get_mut() }
            .get(&crate::highlight_defs::ColorKey::new(namespace as i32, hl_id))
            .expect("namespace definition");
        assert!(item.attr_id > 0);
        let attrs = unsafe { crate::highlight::syn_attr2entry(item.attr_id) };
        assert_eq!(attrs.rgb_fg_color, 0x123456);
        assert_eq!(
            attrs.rgb_ae_attr,
            crate::highlight_defs::HL_BOLD as i32
        );
    }

    #[test]
    fn nvim_set_hl_rejects_url_attributes() {
        let _lock = crate::globals::global_state_test_lock();
        let mut err = Error::default();
        unsafe {
            nvim_set_hl(
                4002,
                &b"NeroApiUrlHighlight".to_vec(),
                &crate::highlight::HighlightDict {
                    url: Some(b"https://example.test".to_vec()),
                    ..Default::default()
                },
                &mut err,
            )
        };
        assert_eq!(err.msg.as_deref(), Some("Invalid key: 'url'"));
    }

    #[test]
    fn nvim_get_hl_returns_a_global_definition_by_name() {
        let _lock = crate::globals::global_state_test_lock();
        let name = b"NeroApiReadableHighlight".to_vec();
        let _firstwin = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.firstwin,
                std::ptr::null_mut(),
            )
        };
        let mut set_err = Error::default();
        unsafe {
            nvim_set_hl(
                0,
                &name,
                &crate::highlight::HighlightDict {
                    italic: Some(true),
                    fg: Some(Object::Integer(0xabcdef)),
                    ..Default::default()
                },
                &mut set_err,
            )
        };
        let mut err = Error::default();
        let definition = unsafe {
            nvim_get_hl(
                0,
                &GetHighlightOpts {
                    name: Some(name),
                    ..Default::default()
                },
                &mut err,
            )
        };

        assert!(!set_err.is_set());
        assert!(!err.is_set());
        assert!(definition.iter().any(|pair| {
            pair.key == b"italic" && matches!(pair.value, Object::Boolean(true))
        }));
        assert!(definition.iter().any(|pair| {
            pair.key == b"fg" && matches!(pair.value, Object::Integer(0xabcdef))
        }));
    }

    #[test]
    fn nvim_get_hl_handles_missing_names_and_invalid_ids() {
        let _lock = crate::globals::global_state_test_lock();
        let mut missing_err = Error::default();
        assert!(
            unsafe {
                nvim_get_hl(
                    0,
                    &GetHighlightOpts {
                        create: Some(false),
                        name: Some(b"NeroApiMissingHighlight".to_vec()),
                        ..Default::default()
                    },
                    &mut missing_err,
                )
            }
            .is_empty()
        );
        assert!(!missing_err.is_set());

        let mut id_err = Error::default();
        assert!(
            unsafe {
                nvim_get_hl(
                    0,
                    &GetHighlightOpts {
                        id: Some(i64::from(i32::MAX)),
                        ..Default::default()
                    },
                    &mut id_err,
                )
            }
            .is_empty()
        );
        assert_eq!(id_err.msg.as_deref(), Some("Highlight id out of bounds"));
    }

    #[test]
    fn nvim_get_hl_id_by_name_creates_once_and_reuses_the_id() {
        let _lock = crate::globals::global_state_test_lock();
        let name = b"NeroApiHighlightIdLookup".to_vec();
        let first = unsafe { nvim_get_hl_id_by_name(&name) };
        let second = unsafe { nvim_get_hl_id_by_name(&name) };
        assert!(first > 0);
        assert_eq!(first, second);
    }

    #[test]
    fn nvim_set_vvar_coerces_writable_special_variables() {
        let _lock = crate::globals::global_state_test_lock();
        let dict = crate::eval::vars::get_vimvar_dict();
        let item = unsafe { crate::eval::typval::tv_dict_find(Some(&mut *dict), b"errmsg") }
            .expect("v:errmsg");
        let previous = unsafe { (*item).di_tv.clone() };
        let mut err = Error::default();
        unsafe {
            nvim_set_vvar(
                &b"errmsg".to_vec(),
                &Object::Integer(42),
                &mut err,
            )
        };
        let written = unsafe { (*item).di_tv.clone() };
        unsafe {
            crate::eval::typval::tv_clear_simple(&(*item).di_tv);
            (*item).di_tv = previous;
        }
        assert!(matches!(
            written.value,
            crate::eval::typval_defs::TypvalValue::String(Some(value)) if value == b"42"
        ));
        assert!(!err.is_set());
    }

    #[test]
    fn nvim_get_mark_returns_a_shada_file_mark_tuple() {
        let _lock = crate::globals::global_state_test_lock();
        let marks = unsafe { crate::mark::NAMEDFM.get_mut() };
        let saved = marks.clone();
        let index = crate::mark::mark_global_index(b'Q') as usize;
        marks[index].fmark.mark = crate::pos_defs::PosT {
            lnum: 12,
            col: 4,
            coladd: 0,
        };
        marks[index].fmark.fnum = 0;
        marks[index].fname = Some(b"/tmp/marked".to_vec());
        let mut err = Error::default();
        let result = unsafe { nvim_get_mark(&b"Q".to_vec(), &mut err) };
        *unsafe { crate::mark::NAMEDFM.get_mut() } = saved;
        assert!(!err.is_set());
        assert!(matches!(
            result.as_slice(),
            [
                Object::Integer(12),
                Object::Integer(4),
                Object::Integer(0),
                Object::String(filename)
            ] if filename == b"/tmp/marked"
        ));
    }

    #[test]
    fn nvim_get_mark_rejects_buffer_local_names() {
        let mut err = Error::default();
        assert!(
            unsafe { nvim_get_mark(&b"a".to_vec(), &mut err) }.is_empty()
        );
        assert_eq!(
            err.msg.as_deref(),
            Some("Invalid mark name (must be file/uppercase): 'a'")
        );
    }

    struct HighlightNamespaceGuard {
        global: i32,
        win: i32,
        fast: i32,
        active: i32,
        need_changed: bool,
        firstwin: *mut WinT,
        first_tabpage: *mut TabpageT,
        curtab: *mut TabpageT,
    }

    impl HighlightNamespaceGuard {
        fn set(global: i32, win: i32, fast: i32, active: i32) -> Self {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            let guard = Self {
                global: unsafe { *crate::highlight::NS_HL_GLOBAL.get_mut() },
                win: unsafe { *crate::highlight::NS_HL_WIN.get_mut() },
                fast: unsafe { *crate::highlight::NS_HL_FAST.get_mut() },
                active: unsafe { *crate::highlight::NS_HL_ACTIVE.get_mut() },
                need_changed: globals.need_highlight_changed,
                firstwin: globals.firstwin,
                first_tabpage: globals.first_tabpage,
                curtab: globals.curtab,
            };
            unsafe {
                *crate::highlight::NS_HL_GLOBAL.get_mut() = global;
                *crate::highlight::NS_HL_WIN.get_mut() = win;
                *crate::highlight::NS_HL_FAST.get_mut() = fast;
                *crate::highlight::NS_HL_ACTIVE.get_mut() = active;
            }
            globals.need_highlight_changed = false;
            globals.firstwin = std::ptr::null_mut();
            globals.first_tabpage = std::ptr::null_mut();
            globals.curtab = std::ptr::null_mut();
            guard
        }
    }

    impl Drop for HighlightNamespaceGuard {
        fn drop(&mut self) {
            unsafe {
                *crate::highlight::NS_HL_GLOBAL.get_mut() = self.global;
                *crate::highlight::NS_HL_WIN.get_mut() = self.win;
                *crate::highlight::NS_HL_FAST.get_mut() = self.fast;
                *crate::highlight::NS_HL_ACTIVE.get_mut() = self.active;
                let globals = crate::globals::GLOBALS.get_mut();
                globals.need_highlight_changed = self.need_changed;
                globals.firstwin = self.firstwin;
                globals.first_tabpage = self.first_tabpage;
                globals.curtab = self.curtab;
            }
        }
    }

    #[test]
    fn nvim_get_current_win_returns_curwin_handle() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT { handle: 42, ..Default::default() };
        let win_ptr = std::ptr::addr_of_mut!(win);

        // SAFETY: single-threaded test, GLOBALS restored below.
        unsafe {
            let prev_curwin = crate::globals::GLOBALS.get_mut().curwin;
            crate::globals::GLOBALS.get_mut().curwin = win_ptr;

            assert_eq!(nvim_get_current_win(), 42);

            crate::globals::GLOBALS.get_mut().curwin = prev_curwin;
        }
    }

    #[test]
    fn nvim_get_current_buf_returns_curbuf_handle() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { handle: 43, ..Default::default() };
        let buf_ptr = std::ptr::addr_of_mut!(buf);

        // SAFETY: single-threaded test, GLOBALS restored below.
        unsafe {
            let prev_curbuf = crate::globals::GLOBALS.get_mut().curbuf;
            crate::globals::GLOBALS.get_mut().curbuf = buf_ptr;

            assert_eq!(nvim_get_current_buf(), 43);

            crate::globals::GLOBALS.get_mut().curbuf = prev_curbuf;
        }
    }

    #[test]
    fn nvim_get_current_tabpage_returns_curtab_handle() {
        let _lock = crate::globals::global_state_test_lock();
        let mut tab = TabpageT { handle: 44, ..Default::default() };
        let tab_ptr = std::ptr::addr_of_mut!(tab);

        // SAFETY: single-threaded test, GLOBALS restored below.
        unsafe {
            let prev_curtab = crate::globals::GLOBALS.get_mut().curtab;
            crate::globals::GLOBALS.get_mut().curtab = tab_ptr;

            assert_eq!(nvim_get_current_tabpage(), 44);

            crate::globals::GLOBALS.get_mut().curtab = prev_curtab;
        }
    }

    #[test]
    fn nvim_strwidth_sums_ascii_widths() {
        let mut err = Error::default();
        let text: NvimString = b"hello".to_vec();
        // SAFETY: pure ASCII input, no OPTION_VARS-dependent branch.
        assert_eq!(unsafe { nvim_strwidth(&text, &mut err) }, 5);
        assert!(!err.is_set());
    }

    #[test]
    fn nvim_strwidth_counts_a_double_width_char_as_two() {
        let mut err = Error::default();
        let text: NvimString = "一".as_bytes().to_vec();
        // SAFETY: same as above.
        assert_eq!(unsafe { nvim_strwidth(&text, &mut err) }, 2);
        assert!(!err.is_set());
    }

    #[test]
    fn nvim_strwidth_zero_for_empty_string() {
        let mut err = Error::default();
        let text: NvimString = Vec::new();
        // SAFETY: same as above.
        assert_eq!(unsafe { nvim_strwidth(&text, &mut err) }, 0);
        assert!(!err.is_set());
    }

    /// `text_length_ok` (factored out of `nvim_strwidth` itself)
    /// lets the exact `i32::MAX` boundary be tested directly, without
    /// needing a genuine ~2 GiB `Vec<u8>` allocation to exercise the
    /// real "too long" branch end-to-end.
    #[test]
    fn text_length_ok_true_at_the_i32_max_boundary() {
        assert!(text_length_ok(i32::MAX as usize));
    }

    #[test]
    fn text_length_ok_false_just_past_the_i32_max_boundary() {
        assert!(!text_length_ok(i32::MAX as usize + 1));
    }

    #[test]
    fn nvim_get_hl_ns_returns_global_or_window_namespace() {
        let _lock = crate::globals::global_state_test_lock();
        let _namespace = HighlightNamespaceGuard::set(6, -1, -1, -1);
        let mut err = Error::default();
        assert_eq!(unsafe { nvim_get_hl_ns(None, &mut err) }, 6);
        assert!(!err.is_set());

        let mut win = WinT {
            handle: 42,
            w_ns_hl: 9,
            ..Default::default()
        };
        let win_ptr = std::ptr::addr_of_mut!(win);
        let mut tab = TabpageT::default();
        let tab_ptr = std::ptr::addr_of_mut!(tab);
        unsafe {
            let globals = crate::globals::GLOBALS.get_mut();
            globals.firstwin = win_ptr;
            globals.first_tabpage = tab_ptr;
            globals.curtab = tab_ptr;
        }
        assert_eq!(unsafe { nvim_get_hl_ns(Some(42), &mut err) }, 9);

        assert_eq!(unsafe { nvim_get_hl_ns(Some(99), &mut err) }, 0);
        assert_eq!(err.msg.as_deref(), Some("Invalid window id: 99"));
    }

    #[test]
    fn nvim_set_hl_ns_validates_and_selects_global_namespace_zero() {
        let _lock = crate::globals::global_state_test_lock();
        let _namespace = HighlightNamespaceGuard::set(7, -1, -1, 7);
        let mut err = Error::default();

        unsafe { nvim_set_hl_ns(-1, &mut err) };
        assert_eq!(err.r#type, ErrorType::Validation);
        assert_eq!(err.msg.as_deref(), Some("Invalid 'namespace': -1"));
        assert_eq!(unsafe { *crate::highlight::NS_HL_GLOBAL.get_mut() }, 7);

        err = Error::default();
        unsafe { nvim_set_hl_ns(0, &mut err) };
        assert!(!err.is_set());
        assert_eq!(unsafe { *crate::highlight::NS_HL_GLOBAL.get_mut() }, 0);
        assert_eq!(unsafe { *crate::highlight::NS_HL_ACTIVE.get_mut() }, 0);
        assert!(unsafe { crate::globals::GLOBALS.get_mut() }.need_highlight_changed);
    }

    #[test]
    fn nvim_set_hl_ns_fast_accepts_negative_namespace() {
        let _lock = crate::globals::global_state_test_lock();
        let _namespace = HighlightNamespaceGuard::set(0, -1, 3, 3);

        unsafe { nvim_set_hl_ns_fast(-1) };

        assert_eq!(unsafe { *crate::highlight::NS_HL_FAST.get_mut() }, -1);
        assert_eq!(unsafe { *crate::highlight::NS_HL_ACTIVE.get_mut() }, 0);
    }
}
