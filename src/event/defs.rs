//! Translated concrete callback definitions from
//! `src/nvim/event/defs.h`.

/// Maximum callback argument count (`EVENT_HANDLER_MAX_ARGC`).
pub const EVENT_HANDLER_MAX_ARGC: usize = 10;

/// Event callback (`argv_callback`).
pub type ArgvCallback =
    unsafe fn(argv: *mut *mut std::ffi::c_void);

/// One queued event (`Event`).
#[derive(Debug, Clone, Copy)]
pub struct Event {
    /// Callback, or `None` for `NILEVENT`.
    pub handler: Option<ArgvCallback>,
    /// Opaque callback arguments.
    pub argv: [*mut std::ffi::c_void; EVENT_HANDLER_MAX_ARGC],
}

impl Default for Event {
    fn default() -> Self {
        Self {
            handler: None,
            argv: [std::ptr::null_mut(); EVENT_HANDLER_MAX_ARGC],
        }
    }
}

/// Construct an event from a callback and leading arguments
/// (`event_create`).
#[must_use]
pub fn event_create(
    handler: ArgvCallback,
    arguments: &[*mut std::ffi::c_void],
) -> Event {
    assert!(arguments.len() <= EVENT_HANDLER_MAX_ARGC);
    let mut event = Event {
        handler: Some(handler),
        ..Default::default()
    };
    event.argv[..arguments.len()].copy_from_slice(arguments);
    event
}

/// Parent-queue notification callback (`PutCallback`).
pub type PutCallback = unsafe fn(
    queue: *mut crate::event::multiqueue::MultiQueue,
    data: *mut std::ffi::c_void,
);

#[cfg(test)]
mod tests {
    use super::*;

    unsafe fn callback(_argv: *mut *mut std::ffi::c_void) {}

    #[test]
    fn event_default_is_the_nil_event() {
        let event = Event::default();
        assert!(event.handler.is_none());
        assert!(event.argv.iter().all(|argument| argument.is_null()));
    }

    #[test]
    fn event_create_copies_leading_arguments_and_zero_fills_rest() {
        let mut first_value = 1usize;
        let mut second_value = 2usize;
        let first = std::ptr::addr_of_mut!(first_value).cast();
        let second = std::ptr::addr_of_mut!(second_value).cast();
        let event = event_create(callback, &[first, second]);
        assert!(event.handler.is_some());
        assert_eq!(event.argv[0], first);
        assert_eq!(event.argv[1], second);
        assert!(event.argv[2..].iter().all(|argument| argument.is_null()));
    }
}
