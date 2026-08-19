//! Translated from `src/nvim/api/autocmd.c` (augroup API core).

use crate::api::private::defs::{Error, Integer, NvimString};

/// Typed `Dict(create_augroup)` options.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CreateAugroupOpts {
    pub clear: Option<bool>,
}

/// Create or retrieve an augroup (`nvim_create_augroup`).
///
/// # Safety
/// Mutates shared augroup/autocmd registries and script context.
pub unsafe fn nvim_create_augroup(
    channel_id: u64,
    name: &NvimString,
    opts: CreateAugroupOpts,
    err: &mut Error,
) -> Integer {
    let previous = unsafe { crate::api::private::helpers::api_set_sctx(channel_id) };
    let group = unsafe { crate::autocmd::augroup_add(name) };
    if group == crate::autocmd_defs::augroup::ERROR {
        err.r#type = crate::api::private::defs::ErrorType::Exception;
        err.msg = Some("Failed to set augroup".to_string());
    } else if opts.clear.unwrap_or(true) {
        unsafe { crate::autocmd::augroup_clear(group) };
    }
    unsafe { crate::globals::GLOBALS.get_mut() }.current_sctx = previous;
    i64::from(group)
}

/// Delete an augroup by name (`nvim_del_augroup_by_name`).
///
/// # Safety
/// Mutates shared augroup/autocmd registries.
pub unsafe fn nvim_del_augroup_by_name(name: &NvimString, err: &mut Error) {
    if let Err(message) = unsafe { crate::autocmd::augroup_del(name) } {
        err.r#type = crate::api::private::defs::ErrorType::Exception;
        err.msg = Some(format!("{message}: {}", String::from_utf8_lossy(name)));
    }
}

/// Delete an augroup by ID (`nvim_del_augroup_by_id`).
///
/// # Safety
/// Mutates shared augroup/autocmd registries.
pub unsafe fn nvim_del_augroup_by_id(id: Integer, err: &mut Error) {
    let Some(name) = (if id == 0 {
        None
    } else {
        unsafe { crate::autocmd::augroup_name(id as i32) }
    }) else {
        err.r#type = crate::api::private::defs::ErrorType::Exception;
        err.msg = Some(format!("No such group: {id}"));
        return;
    };
    unsafe { nvim_del_augroup_by_name(&name, err) };
}

/// Delete one autocmd by ID (`nvim_del_autocmd`).
///
/// # Safety
/// Mutates shared autocmd/pattern/callback state.
pub unsafe fn nvim_del_autocmd(id: Integer, err: &mut Error) {
    if id <= 0 {
        err.r#type = crate::api::private::defs::ErrorType::Validation;
        err.msg = Some(format!("Invalid autocmd id: {id}"));
        return;
    }
    if !unsafe { crate::autocmd::autocmd_delete_id(id) } {
        err.r#type = crate::api::private::defs::ErrorType::Exception;
        err.msg = Some("Failed to delete autocmd".to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nvim_create_augroup_reuses_named_groups_and_restores_context() {
        let _lock = crate::globals::global_state_test_lock();
        let name = b"NeroApiCreateAugroup".to_vec();
        let previous = unsafe { crate::globals::GLOBALS.get_mut() }.current_sctx;
        let mut err = Error::default();
        let first = unsafe {
            nvim_create_augroup(77, &name, CreateAugroupOpts::default(), &mut err)
        };
        let second = unsafe {
            nvim_create_augroup(
                77,
                &name,
                CreateAugroupOpts { clear: Some(false) },
                &mut err,
            )
        };
        assert!(first > 0);
        assert_eq!(first, second);
        assert!(!err.is_set());
        assert_eq!(
            unsafe { crate::globals::GLOBALS.get_mut() }.current_sctx,
            previous
        );
    }

    #[test]
    fn nvim_del_augroup_by_name_removes_the_group() {
        let _lock = crate::globals::global_state_test_lock();
        let name = b"NeroApiDeleteAugroupName".to_vec();
        let mut err = Error::default();
        let id = unsafe {
            nvim_create_augroup(
                0,
                &name,
                CreateAugroupOpts { clear: Some(false) },
                &mut err,
            )
        };
        assert!(id > 0);
        unsafe { nvim_del_augroup_by_name(&name, &mut err) };
        assert_eq!(crate::autocmd::augroup_find(&name), crate::autocmd_defs::augroup::ERROR);
        assert!(!err.is_set());
    }

    #[test]
    fn nvim_del_augroup_by_id_removes_the_group() {
        let _lock = crate::globals::global_state_test_lock();
        let name = b"NeroApiDeleteAugroupId".to_vec();
        let mut err = Error::default();
        let id = unsafe {
            nvim_create_augroup(
                0,
                &name,
                CreateAugroupOpts { clear: Some(false) },
                &mut err,
            )
        };
        unsafe { nvim_del_augroup_by_id(id, &mut err) };
        assert_eq!(crate::autocmd::augroup_find(&name), crate::autocmd_defs::augroup::ERROR);
        assert!(!err.is_set());
    }

    #[test]
    fn nvim_del_autocmd_validates_and_reports_missing_ids() {
        let _lock = crate::globals::global_state_test_lock();
        let mut invalid = Error::default();
        unsafe { nvim_del_autocmd(0, &mut invalid) };
        assert_eq!(invalid.msg.as_deref(), Some("Invalid autocmd id: 0"));
        let mut missing = Error::default();
        unsafe { nvim_del_autocmd(i64::MAX, &mut missing) };
        assert_eq!(missing.msg.as_deref(), Some("Failed to delete autocmd"));
    }
}
