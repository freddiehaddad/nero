//! Dependency-free write-buffer functions from
//! `src/nvim/event/wstream.c`.

use crate::event::defs::{WBuffer, WbufferDataFinalizer};

/// Allocate a reference-counted write buffer (`wstream_new_buffer`).
///
/// # Safety
/// `data` must satisfy `callback`'s contract and outlive all
/// `refcount` releases.
#[must_use]
pub unsafe fn wstream_new_buffer(
    data: *mut u8,
    size: usize,
    refcount: usize,
    callback: Option<WbufferDataFinalizer>,
) -> *mut WBuffer {
    assert!(!data.is_null());
    Box::into_raw(Box::new(WBuffer {
        size,
        refcount,
        data,
        callback,
    }))
}

/// Release one write-buffer reference (`wstream_release_wbuffer`).
///
/// # Safety
/// `buffer` must be a live pointer returned by
/// [`wstream_new_buffer`], released exactly `refcount` times.
pub unsafe fn wstream_release_wbuffer(buffer: *mut WBuffer) {
    assert!(!buffer.is_null());
    let refcount = unsafe { &mut (*buffer).refcount };
    assert!(*refcount > 0);
    *refcount -= 1;
    if *refcount != 0 {
        return;
    }
    let buffer = unsafe { Box::from_raw(buffer) };
    if let Some(callback) = buffer.callback {
        unsafe { callback(buffer.data.cast()) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    unsafe fn free_byte(data: *mut std::ffi::c_void) {
        unsafe { drop(Box::from_raw(data.cast::<u8>())) };
    }

    #[test]
    fn write_buffer_runs_finalizer_after_last_reference() {
        let data = Box::into_raw(Box::new(42u8));
        let buffer =
            unsafe { wstream_new_buffer(data, 1, 2, Some(free_byte)) };
        unsafe {
            wstream_release_wbuffer(buffer);
            assert_eq!((*buffer).refcount, 1);
            assert_eq!(*(*buffer).data, 42);
            wstream_release_wbuffer(buffer);
        }
    }

    #[test]
    fn write_buffer_without_finalizer_leaves_borrowed_data_owned() {
        let mut data = 7u8;
        let data_ptr = std::ptr::addr_of_mut!(data);
        let buffer =
            unsafe { wstream_new_buffer(data_ptr, 1, 1, None) };
        unsafe { wstream_release_wbuffer(buffer) };
        assert_eq!(data, 7);
    }
}
