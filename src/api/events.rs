//! Translated from `src/nvim/api/events.c` in full.

use crate::api::private::defs::{
    Error, Integer, NvimString, Object, ObjectType,
};

/// Report an asynchronous API response error
/// (`nvim_error_event`).
pub fn nvim_error_event(
    channel_id: u64,
    _error_type: Integer,
    message: &NvimString,
) {
    let message = String::from_utf8_lossy(message);
    let _ = crate::log::logmsg(
        crate::log::LOGLVL_ERR,
        Some("api/events"),
        None,
        None,
        true,
        &format!("async error on channel {channel_id}: {message}"),
    );
}

/// Handle a host-terminal event sent by the TUI client
/// (`nvim_ui_term_event`).
///
/// # Safety
/// Mutates the shared `v:termresponse` slot and autocmd bookkeeping.
pub unsafe fn nvim_ui_term_event(
    channel_id: u64,
    event: &NvimString,
    value: &Object,
    err: &mut Error,
) {
    if event.as_slice() != b"termresponse" {
        return;
    }
    let Object::String(termresponse) = value else {
        crate::api::private::validate::api_err_exp(
            err,
            "termresponse",
            crate::api::private::validate::api_typename(
                ObjectType::String,
            ),
            Some(crate::api::private::validate::api_typename(
                value.object_type(),
            )),
        );
        return;
    };

    unsafe {
        crate::eval::vars::set_vim_var_string(
            crate::eval::vars::VimVarIndex::Termresponse,
            Some(termresponse),
        )
    };
    crate::autocmd::do_termresponse_autocmd(termresponse, channel_id);
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TermresponseGuard {
        value: Option<Vec<u8>>,
        changed: bool,
        channel_id: u64,
    }

    impl TermresponseGuard {
        fn save() -> Self {
            let value = match unsafe {
                &(*crate::eval::vars::get_vim_var_tv(
                    crate::eval::vars::VimVarIndex::Termresponse,
                ))
                .value
            } {
                crate::eval::typval_defs::TypvalValue::String(value) => {
                    value.clone()
                }
                other => panic!("expected termresponse String, got {other:?}"),
            };
            let (changed, channel_id) = unsafe {
                crate::autocmd::replace_termresponse_state_for_test(
                    false, 0,
                )
            };
            Self {
                value,
                changed,
                channel_id,
            }
        }
    }

    impl Drop for TermresponseGuard {
        fn drop(&mut self) {
            unsafe {
                crate::eval::vars::set_vim_var_string(
                    crate::eval::vars::VimVarIndex::Termresponse,
                    self.value.as_deref(),
                );
                crate::autocmd::replace_termresponse_state_for_test(
                    self.changed,
                    self.channel_id,
                );
            }
        }
    }

    #[test]
    fn error_event_is_safe_before_logging_is_initialized() {
        nvim_error_event(12, 1, &b"failed".to_vec());
    }

    #[test]
    fn unknown_terminal_events_are_ignored() {
        let mut err = Error::default();
        unsafe {
            nvim_ui_term_event(
                7,
                &b"unknown".to_vec(),
                &Object::Integer(3),
                &mut err,
            )
        };
        assert!(!err.is_set());
    }

    #[test]
    fn termresponse_requires_a_string_payload() {
        let mut err = Error::default();
        unsafe {
            nvim_ui_term_event(
                7,
                &b"termresponse".to_vec(),
                &Object::Integer(3),
                &mut err,
            )
        };
        assert_eq!(
            err.msg.as_deref(),
            Some(
                "Invalid 'termresponse': expected String, got Integer"
            )
        );
    }

    #[test]
    fn termresponse_updates_vvar_and_autocmd_state() {
        let _lock = crate::globals::global_state_test_lock();
        let _state = TermresponseGuard::save();
        let mut err = Error::default();
        unsafe {
            nvim_ui_term_event(
                42,
                &b"termresponse".to_vec(),
                &Object::String(b"\x1b[?1;2c".to_vec()),
                &mut err,
            )
        };

        assert!(!err.is_set());
        assert_eq!(
            unsafe {
                crate::eval::vars::get_vim_var_str(
                    crate::eval::vars::VimVarIndex::Termresponse,
                )
            },
            b"\x1b[?1;2c"
        );
        let state = unsafe {
            crate::autocmd::replace_termresponse_state_for_test(
                false, 0,
            )
        };
        assert_eq!(state, (true, 42));
    }
}
