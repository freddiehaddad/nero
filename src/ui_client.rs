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
}
