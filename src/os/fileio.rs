//! Buffered file I/O from `src/nvim/os/fileio.c`.
//!
//! This first slice covers `fileio_defs.h` and the complete in-memory
//! descriptor path used by `file_open_buffer`.

/// File-open flags (`FileOpenFlags`).
pub mod file_open_flags {
    pub const READ_ONLY: i32 = 1;
    pub const CREATE: i32 = 2;
    pub const WRITE_ONLY: i32 = 4;
    pub const NO_SYMLINK: i32 = 8;
    pub const CREATE_ONLY: i32 = 16;
    pub const TRUNCATE: i32 = 32;
    pub const APPEND: i32 = 64;
    pub const NON_BLOCKING: i32 = 128;
    pub const MKDIR: i32 = 256;
}

/// Read/write buffer size (`kRWBufferSize`).
pub const RW_BUFFER_SIZE: usize = 1024;

/// Buffered file descriptor (`FileDescriptor`).
///
/// The original stores three pointers into one allocation. Rust keeps
/// the allocation in `buffer` and stores equivalent byte offsets.
#[derive(Default)]
pub struct FileDescriptor {
    /// Raw descriptor, or `-1` for an in-memory buffer.
    pub fd: i32,
    /// Owned read/write buffer.
    pub buffer: Vec<u8>,
    /// Current read offset.
    pub read_pos: usize,
    /// Current write offset.
    pub write_pos: usize,
    /// Write mode.
    pub write: bool,
    /// Whether backing input reached EOF.
    pub eof: bool,
    /// Whether EAGAIN should be returned directly.
    pub non_blocking: bool,
    /// Total bytes returned or skipped.
    pub bytes_read: u64,
}

/// Open caller-provided bytes for buffered reading
/// (`file_open_buffer`).
pub fn file_open_buffer(descriptor: &mut FileDescriptor, data: &[u8]) {
    *descriptor = FileDescriptor {
        fd: -1,
        buffer: data.to_vec(),
        read_pos: 0,
        write_pos: data.len(),
        write: false,
        eof: true,
        non_blocking: false,
        bytes_read: 0,
    };
}

/// Whether EOF was reached and all buffered bytes consumed
/// (`file_eof`).
#[must_use]
pub fn file_eof(descriptor: &FileDescriptor) -> bool {
    descriptor.eof && descriptor.read_pos == descriptor.write_pos
}

/// Raw file descriptor (`file_fd`).
#[must_use]
pub fn file_fd(descriptor: &FileDescriptor) -> i32 {
    descriptor.fd
}

/// Read bytes from an in-memory or buffered descriptor (`file_read`).
///
/// File-backed refill is added with the file-opening layer; an
/// in-memory descriptor is already at EOF after its supplied bytes.
pub fn file_read(
    descriptor: &mut FileDescriptor,
    output: &mut [u8],
) -> isize {
    assert!(!descriptor.write);
    let available =
        descriptor.write_pos.saturating_sub(descriptor.read_pos);
    let copied = available.min(output.len());
    output[..copied].copy_from_slice(
        &descriptor.buffer
            [descriptor.read_pos..descriptor.read_pos + copied],
    );
    descriptor.read_pos += copied;
    descriptor.bytes_read += copied as u64;
    if copied == output.len() || descriptor.fd < 0 || descriptor.eof {
        return copied as isize;
    }
    unimplemented!("file_read: file-backed refill needs os_read/readv");
}

/// Borrow `size` already-buffered bytes in place
/// (`file_try_read_buffered`).
#[must_use]
pub fn file_try_read_buffered(
    descriptor: &mut FileDescriptor,
    size: usize,
) -> Option<&[u8]> {
    let available =
        descriptor.write_pos.saturating_sub(descriptor.read_pos);
    if available < size {
        return None;
    }
    let start = descriptor.read_pos;
    descriptor.read_pos += size;
    descriptor.bytes_read += size as u64;
    Some(&descriptor.buffer[start..start + size])
}

/// Skip bytes by consuming buffered input (`file_skip`).
pub fn file_skip(
    descriptor: &mut FileDescriptor,
    size: usize,
) -> isize {
    assert!(!descriptor.write);
    let available =
        descriptor.write_pos.saturating_sub(descriptor.read_pos);
    let skipped = available.min(size);
    descriptor.read_pos += skipped;
    descriptor.bytes_read += skipped as u64;
    if skipped == size || descriptor.fd < 0 || descriptor.eof {
        return skipped as isize;
    }
    unimplemented!("file_skip: file-backed refill needs os_read");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_buffer_initializes_read_only_eof_descriptor() {
        let mut descriptor = FileDescriptor::default();
        file_open_buffer(&mut descriptor, b"abcdef");
        assert_eq!(file_fd(&descriptor), -1);
        assert!(!descriptor.write);
        assert!(descriptor.eof);
        assert!(!file_eof(&descriptor));
    }

    #[test]
    fn read_consumes_available_bytes_and_tracks_eof() {
        let mut descriptor = FileDescriptor::default();
        file_open_buffer(&mut descriptor, b"abc");
        let mut output = [0u8; 5];
        assert_eq!(file_read(&mut descriptor, &mut output), 3);
        assert_eq!(&output[..3], b"abc");
        assert_eq!(descriptor.bytes_read, 3);
        assert!(file_eof(&descriptor));
        assert_eq!(file_read(&mut descriptor, &mut output), 0);
    }

    #[test]
    fn try_read_buffered_returns_only_complete_chunks() {
        let mut descriptor = FileDescriptor::default();
        file_open_buffer(&mut descriptor, b"abcdef");
        assert_eq!(
            file_try_read_buffered(&mut descriptor, 3),
            Some(b"abc".as_slice())
        );
        assert_eq!(descriptor.bytes_read, 3);
        assert_eq!(file_try_read_buffered(&mut descriptor, 4), None);
        assert_eq!(
            file_try_read_buffered(&mut descriptor, 3),
            Some(b"def".as_slice())
        );
        assert!(file_eof(&descriptor));
    }

    #[test]
    fn skip_consumes_up_to_the_available_memory_buffer() {
        let mut descriptor = FileDescriptor::default();
        file_open_buffer(&mut descriptor, b"abcdef");
        assert_eq!(file_skip(&mut descriptor, 4), 4);
        assert_eq!(descriptor.bytes_read, 4);
        assert_eq!(file_skip(&mut descriptor, 9), 2);
        assert_eq!(descriptor.bytes_read, 6);
        assert!(file_eof(&descriptor));
    }
}
