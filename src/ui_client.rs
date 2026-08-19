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
}
