//! Translated from `src/nvim/channel.c`.
//!
//! Channel transport, RPC dispatch and event-loop integration are not
//! translated yet. This module starts with the representation-
//! independent helpers used around that machinery.

/// Translated core of `struct Channel`.
///
/// The process/socket stream union, callbacks, RPC state and event
/// queue remain deferred. These fields are the complete state needed
/// by the initial reference-counting helpers.
#[derive(Debug, PartialEq, Eq)]
pub struct ChannelT {
    pub id: u64,
    pub refcount: usize,
    pub did_close_event: bool,
}

/// Buffered callback reader used by channel stdout/stderr
/// (`CallbackReader`).
#[derive(Debug)]
pub struct CallbackReader {
    pub cb: crate::eval::typval_defs::Callback,
    pub self_dict: *mut crate::eval::typval_defs::DictT,
    pub buffer: Vec<Vec<u8>>,
    pub eof: bool,
    pub buffered: bool,
    pub fwd_err: bool,
    pub reader_type: Option<Vec<u8>>,
}

impl Default for CallbackReader {
    fn default() -> Self {
        Self {
            cb: crate::eval::typval_defs::Callback::None,
            self_dict: std::ptr::null_mut(),
            buffer: Vec::new(),
            eof: false,
            buffered: false,
            fwd_err: false,
            reader_type: None,
        }
    }
}

/// Initialize a callback reader's line buffer and stream label
/// (`callback_reader_start`).
pub fn callback_reader_start(reader: &mut CallbackReader, reader_type: &[u8]) {
    reader.buffer = Vec::new();
    reader.reader_type = Some(reader_type.to_vec());
}

/// Add one reference to a channel (`channel_incref`).
pub fn channel_incref(channel: &mut ChannelT) {
    channel.refcount = channel.refcount.wrapping_add(1);
}

/// Three-way comparison of channel IDs (`int64_t_cmp`).
///
/// Written with comparisons rather than subtraction, so the full
/// `i64` range is safe.
#[must_use]
pub fn int64_t_cmp(a: i64, b: i64) -> i32 {
    if a == b {
        0
    } else if a > b {
        1
    } else {
        -1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int64_t_cmp_reports_all_three_orderings() {
        assert_eq!(int64_t_cmp(3, 2), 1);
        assert_eq!(int64_t_cmp(2, 3), -1);
        assert_eq!(int64_t_cmp(3, 3), 0);
    }

    #[test]
    fn int64_t_cmp_handles_the_full_range_without_subtraction_overflow() {
        assert_eq!(int64_t_cmp(i64::MAX, i64::MIN), 1);
        assert_eq!(int64_t_cmp(i64::MIN, i64::MAX), -1);
    }

    #[test]
    fn int64_t_cmp_drives_ascending_channel_id_sorting() {
        let mut ids = [i64::MAX, 7, -3, i64::MIN, 0];
        ids.sort_by(|a, b| int64_t_cmp(*a, *b).cmp(&0));
        assert_eq!(ids, [i64::MIN, -3, 0, 7, i64::MAX]);
    }

    #[test]
    fn channel_core_state_preserves_identity_and_lifetime_fields() {
        let channel = ChannelT {
            id: 42,
            refcount: 1,
            did_close_event: false,
        };
        assert_eq!(channel.id, 42);
        assert_eq!(channel.refcount, 1);
        assert!(!channel.did_close_event);
    }

    #[test]
    fn channel_incref_increments_only_the_reference_count() {
        let mut channel = ChannelT {
            id: 42,
            refcount: 1,
            did_close_event: false,
        };
        channel_incref(&mut channel);
        assert_eq!(
            channel,
            ChannelT {
                id: 42,
                refcount: 2,
                did_close_event: false,
            }
        );
    }

    #[test]
    fn callback_reader_default_matches_callback_reader_init() {
        let reader = CallbackReader::default();
        assert_eq!(
            reader.cb.kind(),
            crate::eval::typval_defs::CallbackType::None
        );
        assert!(reader.self_dict.is_null());
        assert!(reader.buffer.is_empty());
        assert!(!reader.eof);
        assert!(!reader.buffered);
        assert!(!reader.fwd_err);
        assert!(reader.reader_type.is_none());
    }

    #[test]
    fn callback_reader_start_initializes_buffer_and_type_only() {
        let mut reader = CallbackReader {
            buffer: vec![b"stale".to_vec()],
            eof: true,
            buffered: true,
            ..Default::default()
        };

        callback_reader_start(&mut reader, b"stderr");

        assert!(reader.buffer.is_empty());
        assert_eq!(reader.reader_type.as_deref(), Some(b"stderr".as_slice()));
        assert!(reader.eof);
        assert!(reader.buffered);
    }
}
