//! Translated from `src/nvim/api/options.c` (typed read path).
//!
//! [`nvim_get_option_value`] is complete for global/current-context
//! reads and every target shape already supported by
//! `get_option_value_for`. The `filetype` dummy-buffer path still
//! needs real buffer creation/context switching.

use std::ffi::c_void;

use crate::api::private::defs::{
    Buffer, Error, ErrorType, NvimString, Object, Tabpage, Window,
};
use crate::option_defs::{OptIndex, OptScope};

/// Typed `Dict(option)` arguments.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OptionValueOpts {
    pub buf: Option<Buffer>,
    pub filetype: Option<NvimString>,
    pub scope: Option<NvimString>,
    pub tab: Option<Tabpage>,
    pub win: Option<Window>,
    pub operation: Option<NvimString>,
    pub dry_run: Option<bool>,
}

unsafe fn validate_option_value_args(
    opts: &OptionValueOpts,
    name: &[u8],
    err: &mut Error,
) -> Option<(OptIndex, u32, OptScope, *mut c_void)> {
    let has_target = opts.scope.is_some()
        || opts.buf.is_some()
        || opts.win.is_some()
        || opts.tab.is_some();
    if opts.filetype.is_some() && has_target {
        err.r#type = ErrorType::Validation;
        err.msg = Some("Cannot use 'filetype' with 'scope', 'buf', 'win' or 'tab'".to_string());
        return None;
    }
    if opts.tab.is_some()
        && (opts.win.is_some()
            || opts.buf.is_some()
            || opts.filetype.is_some()
            || opts.scope.is_some())
    {
        err.r#type = ErrorType::Validation;
        err.msg = Some("Cannot use 'tab' with 'win', 'buf', 'filetype' or 'scope'".to_string());
        return None;
    }
    if opts.win.is_some() && opts.buf.is_some() {
        err.r#type = ErrorType::Validation;
        err.msg = Some("Cannot use 'buf' with 'win'".to_string());
        return None;
    }
    if opts.filetype.is_some() {
        unimplemented!(
            "nvim_get_option_value: filetype defaults need do_ft_buf and real context switching"
        );
    }

    let mut opt_flags = 0;
    if let Some(scope) = &opts.scope {
        if scope == b"local" {
            opt_flags = crate::option_defs::opt_set_flags::OPT_LOCAL;
        } else if scope == b"global" {
            opt_flags = crate::option_defs::opt_set_flags::OPT_GLOBAL;
        } else {
            err.r#type = ErrorType::Validation;
            err.msg = Some("Invalid 'scope': expected 'local' or 'global'".to_string());
            return None;
        }
    }
    let mut scope = OptScope::Global;
    let mut from = std::ptr::null_mut();
    if let Some(win) = opts.win {
        scope = OptScope::Win;
        from = unsafe { crate::api::private::helpers::find_window_by_handle(win, err) }.cast();
        if err.is_set() {
            return None;
        }
    }
    if let Some(buf) = opts.buf {
        if opt_flags == crate::option_defs::opt_set_flags::OPT_GLOBAL {
            err.r#type = ErrorType::Validation;
            err.msg = Some("Cannot use 'buf' with global scope".to_string());
            return None;
        }
        opt_flags = crate::option_defs::opt_set_flags::OPT_LOCAL;
        scope = OptScope::Buf;
        from = unsafe { crate::api::private::helpers::find_buffer_by_handle(buf, err) }.cast();
        if err.is_set() {
            return None;
        }
    }
    if let Some(tab) = opts.tab {
        scope = OptScope::Tab;
        from = unsafe { crate::api::private::helpers::find_tab_by_handle(tab, err) }.cast();
        if err.is_set() {
            return None;
        }
    }

    let opt_idx = crate::option::find_option(name);
    if opt_idx == OptIndex::Invalid {
        err.r#type = ErrorType::Validation;
        err.msg = Some(format!("Unknown option '{}'", String::from_utf8_lossy(name)));
        return None;
    }
    if scope == OptScope::Tab && !crate::option::option_has_scope(opt_idx, OptScope::Tab) {
        err.r#type = ErrorType::Validation;
        err.msg = Some(format!(
            "Cannot use 'tab' with '{}'",
            String::from_utf8_lossy(name)
        ));
        return None;
    }
    if matches!(scope, OptScope::Buf | OptScope::Win)
        && !crate::option::option_has_scope(opt_idx, scope)
    {
        let target = if scope == OptScope::Buf { "buf" } else { "win" };
        err.r#type = ErrorType::Validation;
        err.msg = Some(format!(
            "'{target}' cannot be passed for option '{}'",
            String::from_utf8_lossy(name)
        ));
        return None;
    }
    Some((opt_idx, opt_flags, scope, from))
}

/// Get an option value (`nvim_get_option_value`).
///
/// # Safety
/// Forwarded from handle lookup and
/// [`crate::option::get_option_value_for`].
#[must_use]
pub unsafe fn nvim_get_option_value(
    name: &NvimString,
    opts: &OptionValueOpts,
    err: &mut Error,
) -> Object {
    let Some((opt_idx, opt_flags, scope, from)) =
        (unsafe { validate_option_value_args(opts, name, err) })
    else {
        return Object::Nil;
    };
    let value =
        unsafe { crate::option::get_option_value_for(opt_idx, opt_flags, scope, from) };
    if value == crate::option_defs::OptVal::Nil {
        err.r#type = ErrorType::Validation;
        err.msg = Some(format!("Invalid 'option': '{}'", String::from_utf8_lossy(name)));
        Object::Nil
    } else {
        crate::option::optval_as_object(value)
    }
}

fn option_info_dict(opt_idx: OptIndex) -> crate::api::private::defs::Dict {
    use crate::api::private::defs::KeyValuePair;
    use crate::option_defs::{OptValType, opt_flags};
    let option = crate::option::get_option(opt_idx);
    let scope = if crate::option::option_has_scope(opt_idx, OptScope::Buf) {
        b"buf".as_slice()
    } else if crate::option::option_has_scope(opt_idx, OptScope::Win) {
        b"win".as_slice()
    } else if crate::option::option_has_scope(opt_idx, OptScope::Tab) {
        b"tab".as_slice()
    } else {
        b"global".as_slice()
    };
    let type_name = match crate::option::option_get_type(opt_idx) {
        OptValType::Nil => b"nil".as_slice(),
        OptValType::Boolean => b"boolean".as_slice(),
        OptValType::Number => b"number".as_slice(),
        OptValType::String => b"string".as_slice(),
    };
    let mut result = Vec::with_capacity(13);
    let mut put = |key: &[u8], value: Object| {
        result.push(KeyValuePair {
            key: key.to_vec(),
            value,
        });
    };
    put(b"name", Object::String(option.fullname.to_vec()));
    put(
        b"shortname",
        Object::String(option.shortname.unwrap_or_default().to_vec()),
    );
    put(b"scope", Object::String(scope.to_vec()));
    put(
        b"global_local",
        Object::Boolean(crate::option::option_is_global_local(opt_idx)),
    );
    put(
        b"commalist",
        Object::Boolean(option.flags & opt_flags::COMMA != 0),
    );
    put(
        b"flaglist",
        Object::Boolean(option.flags & opt_flags::FLAG_LIST != 0),
    );
    put(
        b"was_set",
        Object::Boolean(option.flags & opt_flags::WAS_SET != 0),
    );
    put(
        b"last_set_sid",
        Object::Integer(i64::from(option.script_ctx.sc_sid)),
    );
    put(
        b"last_set_linenr",
        Object::Integer(i64::from(option.script_ctx.sc_lnum)),
    );
    put(
        b"last_set_chan",
        Object::Integer(option.script_ctx.sc_chan as i64),
    );
    put(b"type", Object::String(type_name.to_vec()));
    put(
        b"default",
        crate::option::optval_as_object(option.def_val.clone()),
    );
    put(
        b"allows_duplicates",
        Object::Boolean(option.flags & opt_flags::NO_DUP == 0),
    );
    result
}

/// Get metadata for every option (`nvim_get_all_options_info`).
#[must_use]
pub fn nvim_get_all_options_info() -> crate::api::private::defs::Dict {
    (0..crate::option_defs::OPT_COUNT)
        .filter_map(OptIndex::from_index)
        .map(|opt_idx| {
            let option = crate::option::get_option(opt_idx);
            crate::api::private::defs::KeyValuePair {
                key: option.fullname.to_vec(),
                value: Object::Dict(option_info_dict(opt_idx)),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct CurrentOptionContext {
        buf: *mut crate::buffer_defs::BufT,
        win: *mut crate::buffer_defs::WinT,
        syn: *mut crate::buffer_defs::SynblockT,
        old_buf: *mut crate::buffer_defs::BufT,
        old_win: *mut crate::buffer_defs::WinT,
    }

    impl CurrentOptionContext {
        fn new() -> Self {
            let buf = Box::into_raw(Box::new(crate::buffer_defs::BufT {
                b_p_ts: 7,
                ..Default::default()
            }));
            let syn = Box::into_raw(Box::new(crate::buffer_defs::SynblockT::default()));
            let win = Box::into_raw(Box::new(crate::buffer_defs::WinT {
                w_buffer: buf,
                w_s: syn,
                ..Default::default()
            }));
            let globals = crate::globals::GLOBALS.as_ptr();
            let (old_buf, old_win) = unsafe { ((*globals).curbuf, (*globals).curwin) };
            unsafe {
                (*globals).curbuf = buf;
                (*globals).curwin = win;
            }
            Self {
                buf,
                win,
                syn,
                old_buf,
                old_win,
            }
        }
    }

    impl Drop for CurrentOptionContext {
        fn drop(&mut self) {
            let globals = crate::globals::GLOBALS.as_ptr();
            unsafe {
                (*globals).curbuf = self.old_buf;
                (*globals).curwin = self.old_win;
                drop(Box::from_raw(self.win));
                drop(Box::from_raw(self.syn));
                drop(Box::from_raw(self.buf));
            }
        }
    }

    #[test]
    fn nvim_get_option_value_reads_effective_and_global_values() {
        let _lock = crate::globals::global_state_test_lock();
        let _context = CurrentOptionContext::new();
        let old_ts = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ts;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ts = 11;
        let mut effective_err = Error::default();
        let effective = unsafe {
            nvim_get_option_value(
                &b"tabstop".to_vec(),
                &OptionValueOpts::default(),
                &mut effective_err,
            )
        };
        let mut global_err = Error::default();
        let global = unsafe {
            nvim_get_option_value(
                &b"tabstop".to_vec(),
                &OptionValueOpts {
                    scope: Some(b"global".to_vec()),
                    ..Default::default()
                },
                &mut global_err,
            )
        };
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ts = old_ts;
        assert!(matches!(effective, Object::Integer(7)));
        assert!(matches!(global, Object::Integer(11)));
        assert!(!effective_err.is_set());
        assert!(!global_err.is_set());
    }

    #[test]
    fn nvim_get_option_value_rejects_unknown_options_and_conflicting_targets() {
        let _lock = crate::globals::global_state_test_lock();
        let _context = CurrentOptionContext::new();
        let mut unknown = Error::default();
        assert!(matches!(
            unsafe {
                nvim_get_option_value(
                    &b"nero_missing_option".to_vec(),
                    &OptionValueOpts::default(),
                    &mut unknown,
                )
            },
            Object::Nil
        ));
        assert_eq!(
            unknown.msg.as_deref(),
            Some("Unknown option 'nero_missing_option'")
        );
        let mut conflict = Error::default();
        let _ = unsafe {
            nvim_get_option_value(
                &b"tabstop".to_vec(),
                &OptionValueOpts {
                    buf: Some(0),
                    win: Some(0),
                    ..Default::default()
                },
                &mut conflict,
            )
        };
        assert_eq!(conflict.msg.as_deref(), Some("Cannot use 'buf' with 'win'"));
    }

    #[test]
    fn nvim_get_all_options_info_exports_every_option() {
        let options = nvim_get_all_options_info();
        assert_eq!(options.len(), crate::option_defs::OPT_COUNT);
        let tabstop = options
            .iter()
            .find(|item| item.key == b"tabstop")
            .expect("tabstop metadata");
        let Object::Dict(metadata) = &tabstop.value else {
            panic!("metadata dict")
        };
        assert!(metadata
            .iter()
            .any(|item| item.key == b"shortname" && matches!(&item.value, Object::String(value) if value == b"ts")));
        assert!(metadata
            .iter()
            .any(|item| item.key == b"scope" && matches!(&item.value, Object::String(value) if value == b"buf")));
        assert!(metadata
            .iter()
            .any(|item| item.key == b"type" && matches!(&item.value, Object::String(value) if value == b"number")));
    }
}
