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

impl Default for FileDescriptor {
    fn default() -> Self {
        Self {
            fd: -1,
            buffer: Vec::new(),
            read_pos: 0,
            write_pos: 0,
            write: false,
            eof: false,
            non_blocking: false,
            bytes_read: 0,
        }
    }
}

fn open_flags(flags: i32) -> (i32, bool) {
    let write = flags
        & (file_open_flags::CREATE
            | file_open_flags::CREATE_ONLY
            | file_open_flags::TRUNCATE
            | file_open_flags::APPEND
            | file_open_flags::WRITE_ONLY)
        != 0;
    assert!(
        !write || flags & file_open_flags::READ_ONLY == 0,
        "simultaneous read/write is unsupported"
    );
    let mut system = if write {
        libc::O_WRONLY
    } else {
        libc::O_RDONLY
    };
    if flags & file_open_flags::CREATE != 0 {
        system |= libc::O_CREAT;
    }
    if flags & file_open_flags::CREATE_ONLY != 0 {
        assert_eq!(flags & file_open_flags::CREATE, 0);
        system |= libc::O_CREAT | libc::O_EXCL;
    }
    if flags & file_open_flags::TRUNCATE != 0 {
        assert_eq!(flags & file_open_flags::CREATE_ONLY, 0);
        system |= libc::O_TRUNC;
    }
    if flags & file_open_flags::APPEND != 0 {
        assert_eq!(flags & file_open_flags::CREATE_ONLY, 0);
        system |= libc::O_APPEND;
    }
    #[cfg(unix)]
    if flags & file_open_flags::NO_SYMLINK != 0 {
        system |= libc::O_NOFOLLOW;
    }
    (system, write)
}

#[cfg(unix)]
fn system_open(path: &[u8], flags: i32, mode: i32) -> i32 {
    let Ok(path) = std::ffi::CString::new(path) else {
        return -libc::EINVAL;
    };
    let descriptor =
        unsafe { libc::open(path.as_ptr(), flags, mode as libc::mode_t) };
    if descriptor < 0 {
        -std::io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or(libc::EIO)
    } else {
        descriptor
    }
}

#[cfg(windows)]
fn system_open(path: &[u8], flags: i32, mode: i32) -> i32 {
    #[link(name = "ucrt")]
    unsafe extern "C" {
        fn _wopen(path: *const u16, flags: i32, mode: i32) -> i32;
    }
    let Ok(path) = std::str::from_utf8(path) else {
        return -libc::EINVAL;
    };
    let mut path: Vec<u16> = path.encode_utf16().collect();
    path.push(0);
    let descriptor = unsafe { _wopen(path.as_ptr(), flags, mode) };
    if descriptor < 0 {
        -std::io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or(libc::EIO)
    } else {
        descriptor
    }
}

#[cfg(unix)]
fn system_close(fd: i32) -> i32 {
    if unsafe { libc::close(fd) } == 0 {
        0
    } else {
        -std::io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or(libc::EIO)
    }
}

#[cfg(unix)]
fn system_write(fd: i32, data: &[u8], non_blocking: bool) -> isize {
    let mut written = 0usize;
    while written < data.len() {
        let result = unsafe {
            libc::write(
                fd,
                data[written..].as_ptr().cast(),
                data.len() - written,
            )
        };
        if result > 0 {
            written += result as usize;
            if non_blocking {
                break;
            }
            continue;
        }
        if result == 0 {
            break;
        }
        let error = std::io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or(libc::EIO);
        if error == libc::EINTR {
            continue;
        }
        return if written == 0 {
            -(error as isize)
        } else {
            written as isize
        };
    }
    written as isize
}

#[cfg(windows)]
fn system_write(fd: i32, data: &[u8], non_blocking: bool) -> isize {
    #[link(name = "ucrt")]
    unsafe extern "C" {
        fn _write(fd: i32, buffer: *const std::ffi::c_void, count: u32) -> i32;
    }
    let mut written = 0usize;
    while written < data.len() {
        let count = (data.len() - written).min(u32::MAX as usize) as u32;
        let result = unsafe {
            _write(fd, data[written..].as_ptr().cast(), count)
        };
        if result > 0 {
            written += result as usize;
            if non_blocking {
                break;
            }
            continue;
        }
        if result == 0 {
            break;
        }
        return if written == 0 { -1 } else { written as isize };
    }
    written as isize
}

#[cfg(unix)]
fn system_fsync(fd: i32) -> i32 {
    if unsafe { libc::fsync(fd) } == 0 {
        0
    } else {
        -std::io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or(libc::EIO)
    }
}

#[cfg(windows)]
fn system_fsync(fd: i32) -> i32 {
    #[link(name = "ucrt")]
    unsafe extern "C" {
        fn _commit(fd: i32) -> i32;
    }
    if unsafe { _commit(fd) } == 0 {
        0
    } else {
        -1
    }
}

#[cfg(windows)]
fn system_close(fd: i32) -> i32 {
    #[link(name = "ucrt")]
    unsafe extern "C" {
        fn _close(fd: i32) -> i32;
    }
    if unsafe { _close(fd) } == 0 {
        0
    } else {
        -std::io::Error::last_os_error()
            .raw_os_error()
            .unwrap_or(libc::EIO)
    }
}

/// Wrap an existing raw descriptor (`file_open_fd`).
pub fn file_open_fd(
    descriptor: &mut FileDescriptor,
    fd: i32,
    flags: i32,
) -> i32 {
    let (_, write) = open_flags(flags);
    let non_blocking = flags & file_open_flags::NON_BLOCKING != 0;
    assert!(!write || !non_blocking);
    *descriptor = FileDescriptor {
        fd,
        buffer: vec![0; crate::memory_defs::ARENA_BLOCK_SIZE],
        read_pos: 0,
        write_pos: 0,
        write,
        eof: false,
        non_blocking,
        bytes_read: 0,
    };
    0
}

/// Open a filesystem path (`file_open`).
pub fn file_open(
    descriptor: &mut FileDescriptor,
    filename: &[u8],
    flags: i32,
    mode: i32,
) -> i32 {
    if flags & file_open_flags::MKDIR != 0 {
        let path = String::from_utf8_lossy(filename);
        if std::fs::create_dir_all(path.as_ref()).is_err() {
            return -1;
        }
    }
    let (system_flags, _) = open_flags(flags);
    let fd = system_open(filename, system_flags, mode);
    if fd < 0 {
        return fd;
    }
    file_open_fd(descriptor, fd, flags)
}

/// Close a read descriptor (`file_close`'s dependency-free path).
pub fn file_close(
    descriptor: &mut FileDescriptor,
    do_fsync: bool,
) -> i32 {
    if descriptor.fd < 0 {
        return 0;
    }
    let flush_error = if do_fsync {
        file_fsync(descriptor)
    } else {
        file_flush(descriptor)
    };
    let result = system_close(descriptor.fd);
    descriptor.fd = -1;
    descriptor.buffer.clear();
    descriptor.read_pos = 0;
    descriptor.write_pos = 0;
    if result != 0 {
        result
    } else {
        flush_error
    }
}

/// Flush buffered modifications (`file_flush`).
pub fn file_flush(descriptor: &mut FileDescriptor) -> i32 {
    if !descriptor.write {
        return 0;
    }
    let to_write =
        descriptor.write_pos.saturating_sub(descriptor.read_pos);
    if to_write == 0 {
        return 0;
    }
    let result = system_write(
        descriptor.fd,
        &descriptor.buffer
            [descriptor.read_pos..descriptor.write_pos],
        descriptor.non_blocking,
    );
    descriptor.read_pos = 0;
    descriptor.write_pos = 0;
    if result == to_write as isize {
        0
    } else if result >= 0 {
        -libc::EIO
    } else {
        result as i32
    }
}

/// Flush and synchronize modifications (`file_fsync`).
pub fn file_fsync(descriptor: &mut FileDescriptor) -> i32 {
    if !descriptor.write {
        return 0;
    }
    let flush = file_flush(descriptor);
    if flush != 0 {
        return flush;
    }
    let sync = system_fsync(descriptor.fd);
    if matches!(
        sync,
        value if value == -libc::EINVAL
            || value == -libc::EROFS
            || value == -libc::ENOTSUP
    ) {
        0
    } else {
        sync
    }
}

/// Write bytes through the descriptor buffer (`file_write`).
pub fn file_write(
    descriptor: &mut FileDescriptor,
    data: &[u8],
) -> isize {
    assert!(descriptor.write);
    let space = descriptor.buffer.len() - descriptor.write_pos;
    if data.len() < space {
        let end = descriptor.write_pos + data.len();
        descriptor.buffer[descriptor.write_pos..end]
            .copy_from_slice(data);
        descriptor.write_pos = end;
        return data.len() as isize;
    }
    let flush = file_flush(descriptor);
    if flush < 0 {
        return flush as isize;
    }
    if data.len() < descriptor.buffer.len() {
        descriptor.buffer[..data.len()].copy_from_slice(data);
        descriptor.write_pos = data.len();
        return data.len() as isize;
    }
    let written =
        system_write(descriptor.fd, data, descriptor.non_blocking);
    if written >= 0 && written != data.len() as isize {
        -(libc::EIO as isize)
    } else {
        written
    }
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

    struct ScratchFile(std::path::PathBuf);

    impl ScratchFile {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "nero_os_fileio_{name}_{}.txt",
                std::process::id(),
            ));
            std::fs::write(&path, b"contents").unwrap();
            Self(path)
        }
    }

    impl Drop for ScratchFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call open/close FFI")]
    fn file_open_wraps_and_closes_a_read_descriptor() {
        let scratch = ScratchFile::new("read");
        let mut descriptor = FileDescriptor::default();
        let path = scratch.0.to_string_lossy();

        assert_eq!(
            file_open(
                &mut descriptor,
                path.as_bytes(),
                file_open_flags::READ_ONLY,
                0,
            ),
            0
        );
        assert!(file_fd(&descriptor) >= 0);
        assert!(!descriptor.write);
        assert_eq!(descriptor.buffer.len(), crate::memory_defs::ARENA_BLOCK_SIZE);
        assert_eq!(file_close(&mut descriptor, false), 0);
        assert_eq!(file_fd(&descriptor), -1);
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call open/write/fsync FFI")]
    fn buffered_write_flush_and_fsync_persist_content() {
        let scratch = ScratchFile::new("write");
        let mut descriptor = FileDescriptor::default();
        let path = scratch.0.to_string_lossy();
        assert_eq!(
            file_open(
                &mut descriptor,
                path.as_bytes(),
                file_open_flags::WRITE_ONLY
                    | file_open_flags::TRUNCATE,
                0o600,
            ),
            0
        );

        assert_eq!(file_write(&mut descriptor, b"hello"), 5);
        assert_eq!(file_flush(&mut descriptor), 0);
        assert_eq!(file_write(&mut descriptor, b" world"), 6);
        assert_eq!(file_close(&mut descriptor, true), 0);

        assert_eq!(std::fs::read(&scratch.0).unwrap(), b"hello world");
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call open/write/close FFI")]
    fn large_write_bypasses_internal_buffer() {
        let scratch = ScratchFile::new("large");
        let mut descriptor = FileDescriptor::default();
        let path = scratch.0.to_string_lossy();
        assert_eq!(
            file_open(
                &mut descriptor,
                path.as_bytes(),
                file_open_flags::WRITE_ONLY
                    | file_open_flags::TRUNCATE,
                0o600,
            ),
            0
        );
        let data = vec![b'x'; crate::memory_defs::ARENA_BLOCK_SIZE];
        assert_eq!(file_write(&mut descriptor, &data), data.len() as isize);
        assert_eq!(descriptor.write_pos, 0);
        assert_eq!(file_close(&mut descriptor, false), 0);
        assert_eq!(std::fs::read(&scratch.0).unwrap(), data);
    }
}
