//! Translated from `src/nvim/api/deprecated.c` (highlight lookup and
//! read-only buffer-line compatibility subsets).
//!
//! The deprecated highlight-by-ID/name wrappers are retained because
//! they are public API compatibility surfaces. Other functions in the
//! source file remain with their respective not-yet-translated API
//! subsystems.

use crate::api::private::defs::{
    Boolean, Buffer, Dict, Error, ErrorType, Integer, NvimString, StringArray,
};

fn convert_index(index: Integer) -> Integer {
    if index < 0 {
        index - 1
    } else {
        index
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
