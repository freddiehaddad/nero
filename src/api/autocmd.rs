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
}
