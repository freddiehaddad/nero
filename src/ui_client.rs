//! Translated from `src/nvim/ui_client.c` (client state core).
//!
//! RPC transport and TUI event handlers remain with their respective
//! untranslated subsystems. The process-lifetime client state and
//! size update behavior are represented here.

use crate::globals::GlobalCell;

pub static UI_CLIENT_CHANNEL_ID: GlobalCell<u64> = GlobalCell::new(0);
pub static UI_CLIENT_ATTACHED: GlobalCell<bool> = GlobalCell::new(false);
pub static UI_CLIENT_FORWARD_STDIN: GlobalCell<bool> = GlobalCell::new(false);
static TUI_WIDTH: GlobalCell<i32> = GlobalCell::new(0);
static TUI_HEIGHT: GlobalCell<i32> = GlobalCell::new(0);
pub static UI_CLIENT_ERROR_EXIT: GlobalCell<i32> = GlobalCell::new(0);
static RESTART_ARGS: std::sync::LazyLock<
    GlobalCell<crate::api::private::defs::Array>,
> = std::sync::LazyLock::new(|| GlobalCell::new(Vec::new()));
static RESTART_PENDING: GlobalCell<bool> = GlobalCell::new(false);
static RESTART_ARGS_AFTER_CRASH_EXIT: std::sync::LazyLock<
    GlobalCell<crate::api::private::defs::Array>,
> = std::sync::LazyLock::new(|| GlobalCell::new(Vec::new()));

/// Update the UI client's known dimensions (`ui_client_set_size`).
///
/// # Safety
/// Mutates shared UI-client state.
pub unsafe fn ui_client_set_size(width: i32, height: i32) {
    if unsafe { *UI_CLIENT_ATTACHED.get_mut() } {
        unimplemented!("ui_client_set_size: attached-client resize needs rpc_send_event");
    }
    unsafe {
        *TUI_WIDTH.get_mut() = width;
        *TUI_HEIGHT.get_mut() = height;
    }
}

/// Current dimensions known by the embedded UI client.
///
/// # Safety
/// Reads shared UI-client state.
#[must_use]
pub unsafe fn ui_client_size() -> (i32, i32) {
    unsafe { (*TUI_WIDTH.get_mut(), *TUI_HEIGHT.get_mut()) }
}

/// Reject synchronous `redraw` requests (`handle_ui_client_redraw`).
#[must_use]
pub fn handle_ui_client_redraw(
    _channel_id: u64,
    _args: &crate::api::private::defs::Array,
    error: &mut crate::api::private::defs::Error,
) -> crate::api::private::defs::Object {
    error.r#type = crate::api::private::defs::ErrorType::Validation;
    error.msg = Some("'redraw' cannot be sent as a request".to_string());
    crate::api::private::defs::Object::Nil
}

/// Handle the UI client's `error_exit` event
/// (`ui_client_event_error_exit`).
///
/// # Safety
/// Mutates shared UI-client exit state.
pub unsafe fn ui_client_event_error_exit(args: &crate::api::private::defs::Array) {
    let Some(crate::api::private::defs::Object::Integer(status)) = args.first() else {
        return;
    };
    unsafe { *UI_CLIENT_ERROR_EXIT.get_mut() = *status as i32 };
}

/// Save arguments from a UI `restart` event
/// (`ui_client_event_restart`).
///
/// # Safety
/// Replaces shared restart state.
pub unsafe fn ui_client_event_restart(args: &crate::api::private::defs::Array) {
    *unsafe { RESTART_ARGS.get_mut() } = args.clone();
    unsafe { *RESTART_PENDING.get_mut() = true };
}

/// Save fallback restart arguments for an unexpected server exit
/// (`ui_client_event__set_restart_on_crash_exit`).
///
/// # Safety
/// Replaces shared restart state.
#[allow(non_snake_case)]
pub unsafe fn ui_client_event__set_restart_on_crash_exit(
    args: &crate::api::private::defs::Array,
) {
    *unsafe { RESTART_ARGS_AFTER_CRASH_EXIT.get_mut() } = args.clone();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_client_set_size_records_dimensions_while_detached() {
        let _lock = crate::globals::global_state_test_lock();
        let old_attached = unsafe { *UI_CLIENT_ATTACHED.get_mut() };
        let old_size = unsafe { ui_client_size() };
        unsafe { *UI_CLIENT_ATTACHED.get_mut() = false };
        unsafe { ui_client_set_size(120, 40) };
        assert_eq!(unsafe { ui_client_size() }, (120, 40));
        unsafe {
            ui_client_set_size(old_size.0, old_size.1);
            *UI_CLIENT_ATTACHED.get_mut() = old_attached;
        }
    }

    #[test]
    fn handle_ui_client_redraw_returns_validation_error() {
        let mut error = crate::api::private::defs::Error::default();
        assert!(matches!(
            handle_ui_client_redraw(1, &Vec::new(), &mut error),
            crate::api::private::defs::Object::Nil
        ));
        assert_eq!(
            error.msg.as_deref(),
            Some("'redraw' cannot be sent as a request")
        );
    }

    #[test]
    fn ui_client_event_error_exit_records_integer_status() {
        let _lock = crate::globals::global_state_test_lock();
        let old = unsafe { *UI_CLIENT_ERROR_EXIT.get_mut() };
        unsafe {
            ui_client_event_error_exit(&vec![crate::api::private::defs::Object::Integer(7)])
        };
        assert_eq!(unsafe { *UI_CLIENT_ERROR_EXIT.get_mut() }, 7);
        unsafe {
            ui_client_event_error_exit(&vec![crate::api::private::defs::Object::String(
                b"bad".to_vec(),
            )]);
        }
        assert_eq!(unsafe { *UI_CLIENT_ERROR_EXIT.get_mut() }, 7);
        unsafe { *UI_CLIENT_ERROR_EXIT.get_mut() = old };
    }

    #[test]
    fn ui_client_event_restart_copies_arguments_and_marks_pending() {
        let _lock = crate::globals::global_state_test_lock();
        let old_args = std::mem::take(unsafe { RESTART_ARGS.get_mut() });
        let old_pending = unsafe { *RESTART_PENDING.get_mut() };
        let mut args = vec![crate::api::private::defs::Object::String(
            b"server".to_vec(),
        )];
        unsafe { ui_client_event_restart(&args) };
        args.clear();
        assert!(unsafe { *RESTART_PENDING.get_mut() });
        assert!(matches!(
            unsafe { RESTART_ARGS.get_mut() }.as_slice(),
            [crate::api::private::defs::Object::String(value)] if value == b"server"
        ));
        *unsafe { RESTART_ARGS.get_mut() } = old_args;
        unsafe { *RESTART_PENDING.get_mut() = old_pending };
    }

    #[test]
    fn ui_client_crash_restart_event_copies_arguments() {
        let _lock = crate::globals::global_state_test_lock();
        let old = std::mem::take(unsafe { RESTART_ARGS_AFTER_CRASH_EXIT.get_mut() });
        let mut args = vec![crate::api::private::defs::Object::String(
            b"nvim".to_vec(),
        )];
        unsafe { ui_client_event__set_restart_on_crash_exit(&args) };
        args.clear();
        assert!(matches!(
            unsafe { RESTART_ARGS_AFTER_CRASH_EXIT.get_mut() }.as_slice(),
            [crate::api::private::defs::Object::String(value)] if value == b"nvim"
        ));
        *unsafe { RESTART_ARGS_AFTER_CRASH_EXIT.get_mut() } = old;
    }
}
