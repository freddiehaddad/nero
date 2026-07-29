//! Translated from `src/nvim/buffer_updates.c` (tractable core only).
//!
//! `buffer_updates.c` implements `nvim_buf_attach`'s live-update
//! notification stream (`on_lines`/`on_bytes`/`on_changedtick`/
//! `on_detach`/`on_reload` callbacks, both Lua-ref-based and
//! msgpack-RPC-channel-based). Every function that actually SENDS a
//! notification needs the msgpack-RPC channel layer
//! (`rpc_send_event`) and/or the Lua callback-invocation machinery
//! (`nlua_call_ref`), neither translated - not a narrow gap, the
//! entire notification-dispatch mechanism is missing.
//!
//! Translated: `buf_updates_active` (a pure predicate over
//! `BufT.update_channels`/`update_callbacks`, both already real
//! `Vec` fields).
//!
//! `buf_free_callbacks` needs NO Rust equivalent at all: it just
//! releases `update_channels`/`update_callbacks`' own backing storage
//! (`kv_destroy`) plus, per callback, `buffer_update_callbacks_free`'s
//! own Lua-ref-unreffing - but `BufUpdateCallbacks` is a plain `Copy`
//! struct of `LuaRef` handles here (no Lua host exists to unref
//! against), so Rust's own automatic `Vec`/struct drop already does
//! everything this function's translated behavior would need, the
//! same "Rust's ownership model already does the C free dance
//! automatically" pattern already established elsewhere in this
//! crate (e.g. `stl_clear_click_defs`).
//!
//! Deferred: everything else in the file - `buf_updates_register`/
//! `_unregister`/`_send_end`/`_send_changes`/`_send_splice`/
//! `_changedtick(_single)`/`_unload` all need the channel/Lua
//! callback-dispatch machinery above.

use crate::buffer_defs::BufT;

/// Whether `buf` has any live update subscribers, either RPC channels
/// or Lua callbacks (`buf_updates_active`).
#[must_use]
pub fn buf_updates_active(buf: &BufT) -> bool {
    !buf.update_channels.is_empty() || !buf.update_callbacks.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer_defs::BufUpdateCallbacks;

    #[test]
    fn inactive_when_both_lists_are_empty() {
        let buf = BufT::default();
        assert!(!buf_updates_active(&buf));
    }

    #[test]
    fn active_when_update_channels_is_nonempty() {
        let mut buf = BufT::default();
        buf.update_channels.push(1);
        assert!(buf_updates_active(&buf));
    }

    #[test]
    fn active_when_update_callbacks_is_nonempty() {
        let mut buf = BufT::default();
        buf.update_callbacks.push(BufUpdateCallbacks::default());
        assert!(buf_updates_active(&buf));
    }

    #[test]
    fn active_when_both_lists_are_nonempty() {
        let mut buf = BufT::default();
        buf.update_channels.push(7);
        buf.update_callbacks.push(BufUpdateCallbacks::default());
        assert!(buf_updates_active(&buf));
    }
}
