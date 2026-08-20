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
//! `buf_free_callbacks`/`buffer_update_callbacks_free` are translated:
//! vector teardown is `Vec::clear`, while Lua unref is inert until a
//! Lua host can create such references.
//!
//! Deferred notification tails (`_send_end`/
//! `_changedtick(_single)`/`_unload`) still need the channel/Lua
//! callback-dispatch machinery above.
//! [`buf_updates_send_splice`] and [`buf_updates_unregister`] are
//! translated only as far as their own guards reach: those guards are
//! real, and the dispatch behind them is `unimplemented!()` since
//! nothing can subscribe yet.

use crate::buffer_defs::BufT;

/// Release Lua references held by one callback record
/// (`buffer_update_callbacks_free`).
///
/// No Lua host exists to create live references, so this is currently
/// the original's real no-reference path.
pub fn buffer_update_callbacks_free(
    _callback: crate::buffer_defs::BufUpdateCallbacks,
) {
}

/// Release every buffer-update channel and callback
/// (`buf_free_callbacks`).
pub fn buf_free_callbacks(buf: &mut BufT) {
    buf.update_channels.clear();
    for callback in buf.update_callbacks.drain(..) {
        buffer_update_callbacks_free(callback);
    }
}

/// Register a buffer update channel or Lua callback
/// (`buf_updates_register`).
///
/// The unloaded-buffer, Lua-callback, and duplicate-channel paths are
/// complete. Sending the initial buffer/changedtick event for a newly
/// registered RPC channel remains gated on `rpc_send_event`.
#[must_use]
pub fn buf_updates_register(
    buf: &mut BufT,
    channel_id: u64,
    callback: crate::buffer_defs::BufUpdateCallbacks,
    send_buffer: bool,
) -> bool {
    if buf.b_ml.ml_mfp.is_null() {
        return false;
    }
    if channel_id == crate::api::private::defs::LUA_INTERNAL_CALL {
        buf.update_callbacks.push(callback);
        if callback.utf_sizes {
            buf.update_need_codepoints = true;
        }
        return true;
    }
    if buf.update_channels.contains(&channel_id) {
        return true;
    }
    buf.update_channels.push(channel_id);
    let mode = if send_buffer {
        "initial lines"
    } else {
        "changedtick"
    };
    unimplemented!(
        "buf_updates_register: sending the {mode} RPC event needs rpc_send_event"
    );
}

/// Notify and remove buffer update subscribers during unload
/// (`buf_updates_unload`).
///
/// Empty/default callback cleanup is complete. Live RPC channels or
/// Lua reload/detach handlers remain gated on their dispatch layers.
pub fn buf_updates_unload(buf: &mut BufT, can_reload: bool) {
    if !buf.update_channels.is_empty() {
        unimplemented!(
            "buf_updates_unload: channel detach events need rpc_send_event"
        );
    }

    let mut retained = Vec::new();
    for callback in buf.update_callbacks.drain(..) {
        let handler = if can_reload && callback.on_reload != -1 {
            Some(("reload", callback.on_reload))
        } else if callback.on_detach != -1 {
            Some(("detach", callback.on_detach))
        } else {
            None
        };
        if let Some((kind, _reference)) = handler {
            unimplemented!(
                "buf_updates_unload: Lua {kind} callbacks need nlua_call_ref"
            );
        }
        if can_reload && callback.on_reload != -1 {
            retained.push(callback);
        } else {
            buffer_update_callbacks_free(callback);
        }
    }
    buf.update_callbacks = retained;
}

/// Notify subscribers that only `b:changedtick` changed
/// (`buf_updates_changedtick`).
///
/// Empty/default callback traversal is complete. Live channel and Lua
/// callback dispatch remains gated on their respective runtimes.
pub fn buf_updates_changedtick(buf: &mut BufT) {
    if !buf.update_channels.is_empty() {
        unimplemented!(
            "buf_updates_changedtick: channel events need rpc_send_event"
        );
    }
    let mut retained = Vec::with_capacity(buf.update_callbacks.len());
    for callback in buf.update_callbacks.drain(..) {
        if callback.on_changedtick != -1 {
            unimplemented!(
                "buf_updates_changedtick: Lua callbacks need nlua_call_ref"
            );
        }
        retained.push(callback);
    }
    buf.update_callbacks = retained;
}

/// Whether `buf` has any live update subscribers, either RPC channels
/// or Lua callbacks (`buf_updates_active`).
#[must_use]
pub fn buf_updates_active(buf: &BufT) -> bool {
    !buf.update_channels.is_empty() || !buf.update_callbacks.is_empty()
}

/// Remove `channelid` from `buf`'s live-update subscribers
/// (`buf_updates_unregister`).
///
/// Every occurrence is removed, though a channel should never appear
/// more than once. When nothing was removed the buffer is left
/// untouched and no notification is sent.
///
/// # Translation note
/// The original compacts the kvec in place, shrinks `size` by the
/// number found, and separately `kv_destroy`/`kv_init`s when the last
/// subscriber goes - all of which is `Vec::retain` here, since a
/// `Vec` that has been emptied is already in exactly the state a
/// fresh `kv_init` leaves behind.
///
/// # Deferred boundary
/// Telling the removed channel the stream has ended
/// (`buf_updates_send_end`) needs the msgpack-RPC channel layer
/// (`rpc_send_event`), which is not translated - see this module's
/// own doc comment. The guard around it is real and faithful: it
/// fires only when a channel was actually removed, which cannot
/// happen today because nothing translated can register one, so
/// `update_channels` is always empty and the early return above
/// always wins.
pub fn buf_updates_unregister(buf: &mut BufT, channelid: u64) {
    if buf.update_channels.is_empty() {
        return;
    }

    let before = buf.update_channels.len();
    buf.update_channels.retain(|&id| id != channelid);
    let found = before - buf.update_channels.len();

    if found != 0 {
        unimplemented!(
            "buf_updates_unregister: buf_updates_send_end needs the \
             msgpack-RPC channel layer (rpc_send_event), not translated - \
             unreachable while nothing can register a channel"
        );
    }
}

/// Notify live update subscribers that lines changed
/// (`buf_updates_send_changes`).
///
/// # Scope
///
/// The `ml_flush_deleted_bytes` call happens BEFORE the subscriber
/// check in the original, so it is a real, unconditional side effect
/// and is translated as such: the deleted-byte counters are reset on
/// every call regardless of whether anything is listening.
///
/// The notification itself remains gated on channel/Lua dispatch when
/// subscribers are active.
pub fn buf_updates_send_changes(
    buf: &mut BufT,
    firstline: crate::pos_defs::LinenrT,
    num_added: i64,
    num_removed: i64,
) {
    // Unconditional in the original, ahead of the subscriber check.
    let deleted = crate::memline::ml_flush_deleted_bytes(buf);

    if !buf_updates_active(buf) {
        return;
    }
    if !buf.update_channels.is_empty() {
        unimplemented!(
            "buf_updates_send_changes: line events need rpc_send_event"
        );
    }
    let cmdpreview = unsafe { crate::globals::GLOBALS.get_mut() }.cmdpreview;
    let mut retained = Vec::with_capacity(buf.update_callbacks.len());
    for callback in buf.update_callbacks.drain(..) {
        if callback.on_lines != -1 && (callback.preview || !cmdpreview) {
            let _args = (
                firstline,
                num_added,
                num_removed,
                deleted.0,
                callback.utf_sizes.then_some((deleted.1, deleted.2)),
            );
            unimplemented!(
                "buf_updates_send_changes: Lua line callbacks need nlua_call_ref"
            );
        }
        retained.push(callback);
    }
    buf.update_callbacks = retained;
}

/// Notify live update subscribers of a byte-level splice
/// (`buf_updates_send_splice`).
///
/// The inactive/zero-byte guards and callback preview filtering/
/// retention are complete. Invoking a live `on_bytes` Lua callback
/// remains gated on `nlua_call_ref`.
#[allow(clippy::too_many_arguments)]
pub fn buf_updates_send_splice(
    buf: &mut BufT,
    start_row: i32,
    start_col: crate::pos_defs::ColnrT,
    start_byte: crate::extmark_defs::BcountT,
    old_row: i32,
    old_col: crate::pos_defs::ColnrT,
    old_byte: crate::extmark_defs::BcountT,
    new_row: i32,
    new_col: crate::pos_defs::ColnrT,
    new_byte: crate::extmark_defs::BcountT,
) {
    if !buf_updates_active(buf) || (old_byte == 0 && new_byte == 0) {
        return;
    }
    let cmdpreview = unsafe { crate::globals::GLOBALS.get_mut() }.cmdpreview;
    let mut retained = Vec::with_capacity(buf.update_callbacks.len());
    for callback in buf.update_callbacks.drain(..) {
        if callback.on_bytes != -1 && (callback.preview || !cmdpreview) {
            let _args = (
                start_row, start_col, start_byte, old_row, old_col,
                old_byte, new_row, new_col, new_byte,
            );
            unimplemented!(
                "buf_updates_send_splice: Lua byte callbacks need nlua_call_ref"
            );
        }
        retained.push(callback);
    }
    buf.update_callbacks = retained;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer_defs::BufUpdateCallbacks;

    #[test]
    fn send_splice_returns_quietly_without_subscribers() {
        // Nothing translated can register a channel or callback yet, so
        // this is the always-taken path.
        let mut buf = BufT::default();
        buf_updates_send_splice(&mut buf, 0, 0, 0, 0, 1, 1, 0, 2, 2);
    }

    #[test]
    fn send_splice_returns_quietly_for_a_zero_byte_splice() {
        // The other half of the guard: with subscribers attached but no
        // bytes moved, there is still nothing to report.
        let mut buf = BufT::default();
        buf.update_callbacks.push(BufUpdateCallbacks::default());
        assert!(buf_updates_active(&buf));
        buf_updates_send_splice(&mut buf, 0, 0, 0, 0, 0, 0, 0, 0, 0);
    }

    #[test]
    fn send_splice_keeps_callbacks_without_byte_handlers() {
        let mut buf = BufT::default();
        buf.update_callbacks.push(BufUpdateCallbacks::default());
        buf_updates_send_splice(&mut buf, 0, 0, 0, 0, 1, 1, 0, 2, 2);
        assert_eq!(buf.update_callbacks.len(), 1);
    }

    #[test]
    #[should_panic(expected = "nlua_call_ref")]
    fn send_splice_live_byte_handler_reaches_lua_boundary() {
        let _lock = crate::globals::global_state_test_lock();
        let _preview = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.cmdpreview,
                false,
            )
        };
        let mut buf = BufT::default();
        buf.update_callbacks.push(BufUpdateCallbacks {
            on_bytes: 42,
            ..Default::default()
        });
        buf_updates_send_splice(&mut buf, 0, 0, 0, 0, 1, 1, 0, 2, 2);
    }

    #[test]
    fn send_splice_suppresses_nonpreview_callbacks_during_preview() {
        let _lock = crate::globals::global_state_test_lock();
        let _preview = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.cmdpreview,
                true,
            )
        };
        let mut buf = BufT::default();
        buf.update_callbacks.push(BufUpdateCallbacks {
            on_bytes: 42,
            preview: false,
            ..Default::default()
        });

        buf_updates_send_splice(&mut buf, 0, 0, 0, 0, 1, 1, 0, 2, 2);

        assert_eq!(buf.update_callbacks.len(), 1);
    }

    #[test]
    fn send_changes_flushes_deleted_counts_without_subscribers() {
        let mut buf = BufT {
            deleted_bytes: 7,
            deleted_codepoints: 5,
            deleted_codeunits: 6,
            ..Default::default()
        };

        buf_updates_send_changes(&mut buf, 1, 0, 1);

        assert_eq!(buf.deleted_bytes, 0);
        assert_eq!(buf.deleted_codepoints, 0);
        assert_eq!(buf.deleted_codeunits, 0);
    }

    #[test]
    fn send_changes_keeps_callbacks_without_line_handlers() {
        let mut buf = BufT {
            update_callbacks: vec![BufUpdateCallbacks::default()],
            ..Default::default()
        };
        buf_updates_send_changes(&mut buf, 1, 1, 0);
        assert_eq!(buf.update_callbacks.len(), 1);
    }

    #[test]
    fn send_changes_suppresses_nonpreview_callbacks_during_preview() {
        let _lock = crate::globals::global_state_test_lock();
        let _preview = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.cmdpreview,
                true,
            )
        };
        let mut buf = BufT {
            update_callbacks: vec![BufUpdateCallbacks {
                on_lines: 42,
                preview: false,
                ..Default::default()
            }],
            ..Default::default()
        };

        buf_updates_send_changes(&mut buf, 1, 1, 0);

        assert_eq!(buf.update_callbacks.len(), 1);
    }

    #[test]
    #[should_panic(expected = "nlua_call_ref")]
    fn send_changes_live_line_handler_reaches_lua_boundary() {
        let _lock = crate::globals::global_state_test_lock();
        let _preview = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.cmdpreview,
                false,
            )
        };
        let mut buf = BufT {
            update_callbacks: vec![BufUpdateCallbacks {
                on_lines: 42,
                ..Default::default()
            }],
            ..Default::default()
        };
        buf_updates_send_changes(&mut buf, 1, 1, 0);
    }

    #[test]
    fn unregister_on_a_buffer_with_no_channels_is_a_noop() {
        let mut buf = BufT::default();
        // The early return is what keeps the deferred send_end path
        // unreachable, so it is worth asserting directly.
        buf_updates_unregister(&mut buf, 7);
        assert!(buf.update_channels.is_empty());

        // A buffer with only Lua callbacks still has no channels, so
        // it takes the same early return.
        buf.update_callbacks.push(BufUpdateCallbacks::default());
        buf_updates_unregister(&mut buf, 7);
        assert!(buf.update_channels.is_empty());
        assert_eq!(buf.update_callbacks.len(), 1, "callbacks are untouched");
    }

    #[test]
    fn unregister_of_an_absent_channel_leaves_the_list_alone() {
        let mut buf = BufT {
            update_channels: vec![1, 2, 3],
            ..Default::default()
        };
        // Nothing found means nothing removed and, crucially, no
        // notification - so this must not hit the deferred boundary.
        buf_updates_unregister(&mut buf, 99);
        assert_eq!(buf.update_channels, vec![1, 2, 3]);
    }

    #[test]
    #[should_panic(expected = "not translated")]
    fn unregister_of_a_present_channel_is_unimplemented() {
        // Documents the boundary: telling the removed channel the
        // stream ended needs the RPC layer. Only reachable once a
        // channel can actually be registered.
        let mut buf = BufT {
            update_channels: vec![1, 2, 3],
            ..Default::default()
        };
        buf_updates_unregister(&mut buf, 2);
    }

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

    #[test]
    fn buf_free_callbacks_clears_channels_and_callbacks() {
        let mut buf = BufT {
            update_channels: vec![2, 7],
            update_callbacks: vec![
                BufUpdateCallbacks::default(),
                BufUpdateCallbacks::default(),
            ],
            ..Default::default()
        };

        buf_free_callbacks(&mut buf);

        assert!(buf.update_channels.is_empty());
        assert!(buf.update_callbacks.is_empty());
        assert!(!buf_updates_active(&buf));
    }

    fn loaded_buffer() -> BufT {
        let mut buf = BufT::default();
        buf.b_ml.ml_mfp = std::ptr::NonNull::dangling().as_ptr();
        buf
    }

    #[test]
    fn buf_updates_register_rejects_unloaded_buffers() {
        assert!(!buf_updates_register(
            &mut BufT::default(),
            7,
            BufUpdateCallbacks::default(),
            false,
        ));
    }

    #[test]
    fn buf_updates_register_adds_lua_callbacks_and_utf_requirement() {
        let mut buf = loaded_buffer();
        let callback = BufUpdateCallbacks {
            utf_sizes: true,
            ..Default::default()
        };

        assert!(buf_updates_register(
            &mut buf,
            crate::api::private::defs::LUA_INTERNAL_CALL,
            callback,
            false,
        ));
        assert_eq!(buf.update_callbacks.len(), 1);
        assert!(buf.update_need_codepoints);
    }

    #[test]
    fn buf_updates_register_accepts_an_existing_channel_without_dispatch() {
        let mut buf = loaded_buffer();
        buf.update_channels.push(9);

        assert!(buf_updates_register(
            &mut buf,
            9,
            BufUpdateCallbacks::default(),
            true,
        ));
        assert_eq!(buf.update_channels, vec![9]);
    }

    #[test]
    fn buf_updates_unload_removes_callbacks_without_handlers() {
        let mut buf = BufT {
            update_callbacks: vec![
                BufUpdateCallbacks::default(),
                BufUpdateCallbacks::default(),
            ],
            ..Default::default()
        };

        buf_updates_unload(&mut buf, true);

        assert!(buf.update_callbacks.is_empty());
        assert!(!buf_updates_active(&buf));
    }

    #[test]
    fn buf_updates_unload_is_a_noop_without_subscribers() {
        let mut buf = BufT::default();
        buf_updates_unload(&mut buf, false);
        assert!(!buf_updates_active(&buf));
    }

    #[test]
    fn buf_updates_changedtick_keeps_callbacks_without_handlers() {
        let mut buf = BufT {
            update_callbacks: vec![BufUpdateCallbacks::default()],
            ..Default::default()
        };

        buf_updates_changedtick(&mut buf);

        assert_eq!(buf.update_callbacks.len(), 1);
        assert!(buf_updates_active(&buf));
    }

    #[test]
    fn buf_updates_changedtick_is_a_noop_without_subscribers() {
        let mut buf = BufT::default();
        buf_updates_changedtick(&mut buf);
        assert!(!buf_updates_active(&buf));
    }
}
