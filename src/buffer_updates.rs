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
//! `_send_end`/`_changedtick(_single)`/`_unload` all need the
//! channel/Lua callback-dispatch machinery above.
//! [`buf_updates_send_splice`] and [`buf_updates_unregister`] are
//! translated only as far as their own guards reach: those guards are
//! real, and the dispatch behind them is `unimplemented!()` since
//! nothing can subscribe yet.

use crate::buffer_defs::BufT;

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
/// The notification itself is `unimplemented!()`. Its guard,
/// [`buf_updates_active`], is genuinely always `false` today because
/// nothing translated can register an RPC channel or Lua callback
/// yet, which is the same real, always-taken early return
/// [`buf_updates_send_splice`] relies on.
pub fn buf_updates_send_changes(
    buf: &mut BufT,
    firstline: crate::pos_defs::LinenrT,
    num_added: i64,
    num_removed: i64,
) {
    // Unconditional in the original, ahead of the subscriber check.
    let _deleted = crate::memline::ml_flush_deleted_bytes(buf);

    if !buf_updates_active(buf) {
        return;
    }

    let _ = (firstline, num_added, num_removed);
    unimplemented!(
        "buffer-update dispatch needs the channel/Lua callback machinery, not yet \
         translated; unreachable while nothing can subscribe"
    );
}

/// Notify live update subscribers of a byte-level splice
/// (`buf_updates_send_splice`).
///
/// Always returns without notifying anything in this crate today.
/// That is the original's own first, real check
/// (`if (!buf_updates_active(buf) || (old_byte == 0 && new_byte == 0))
/// return;`), not a shortcut: nothing translated can register an RPC
/// channel or Lua callback yet, so [`buf_updates_active`] is
/// genuinely always `false` and the notification loop is unreachable.
/// This matches this crate's established "translate the real,
/// always-taken early-return path" precedent (`quickfix.rs`'s
/// `qf_mark_adjust`, `fold.rs`'s `checkupdate`).
///
/// The second half of that condition IS reachable and is translated
/// too: a splice that moves no bytes at all notifies nobody even with
/// subscribers attached.
#[allow(clippy::too_many_arguments)]
pub fn buf_updates_send_splice(
    buf: &BufT,
    _start_row: i32,
    _start_col: crate::pos_defs::ColnrT,
    _start_byte: crate::extmark_defs::BcountT,
    _old_row: i32,
    _old_col: crate::pos_defs::ColnrT,
    old_byte: crate::extmark_defs::BcountT,
    _new_row: i32,
    _new_col: crate::pos_defs::ColnrT,
    new_byte: crate::extmark_defs::BcountT,
) {
    if !buf_updates_active(buf) || (old_byte == 0 && new_byte == 0) {
        return;
    }
    // The real per-callback dispatch loop needs `nlua_call_ref`/
    // `rpc_send_event`, neither translated - see this module's own doc
    // comment. Unreachable today for the reason given above.
    unimplemented!(
        "buf_updates_send_splice: the notification dispatch needs the Lua/RPC callback layer, \
         not yet translated - unreachable today since buf_updates_active() is always false"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer_defs::BufUpdateCallbacks;

    #[test]
    fn send_splice_returns_quietly_without_subscribers() {
        // Nothing translated can register a channel or callback yet, so
        // this is the always-taken path.
        let buf = BufT::default();
        buf_updates_send_splice(&buf, 0, 0, 0, 0, 1, 1, 0, 2, 2);
    }

    #[test]
    fn send_splice_returns_quietly_for_a_zero_byte_splice() {
        // The other half of the guard: with subscribers attached but no
        // bytes moved, there is still nothing to report.
        let mut buf = BufT::default();
        buf.update_callbacks.push(BufUpdateCallbacks::default());
        assert!(buf_updates_active(&buf));
        buf_updates_send_splice(&buf, 0, 0, 0, 0, 0, 0, 0, 0, 0);
    }

    #[test]
    #[should_panic(expected = "not yet translated")]
    fn send_splice_with_subscribers_and_real_bytes_is_unimplemented() {
        // Documents the boundary: this is only reachable once buffer
        // updates can actually be registered.
        let mut buf = BufT::default();
        buf.update_callbacks.push(BufUpdateCallbacks::default());
        buf_updates_send_splice(&buf, 0, 0, 0, 0, 1, 1, 0, 2, 2);
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
}
