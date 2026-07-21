//! Translated from `src/nvim/os/fs_defs.h` (partial).
//!
//! Translated: `PathType`, `FileID`, the `NODE_*` constants.
//!
//! Deferred: `FileInfo`/`Directory` - both embed vendored libuv types
//! (`uv_stat_t`/`uv_fs_t`/`uv_dirent_t`) that are out of scope until this
//! project's FFI-vs-Rust-crate decision for libuv is made (see the
//! project plan's deferred decisions, tied to the event loop phase).

/// Currently supports Windows and is extensible (`PathType`).
/// See <https://learn.microsoft.com/en-us/dotnet/standard/io/file-path-formats>
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PathType {
    #[default]
    Unknown = 0,
    /// `foo/bar` or `/foo/bar`
    Generic,
    /// `C:/foo/bar`
    Drive,
    /// `//server/share/foo/bar`
    Unc,
    /// `//?/C:/foo/bar` or `//?/Volume{xxx}/foo/bar`
    Device,
    /// `//?/UNC/server/share/foo/bar`
    DeviceUnc,
}

/// Struct which encapsulates inode/dev_id information (`FileID`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FileID {
    /// The inode of the file.
    pub inode: u64,
    /// The id of the device containing the file.
    pub device_id: u64,
}

impl FileID {
    /// `FILE_ID_EMPTY`
    #[inline]
    pub const fn empty() -> Self {
        FileID { inode: 0, device_id: 0 }
    }
}

// Values returned by `os_nodetype()`.

/// file or directory, check with `os_isdir()`
pub const NODE_NORMAL: i32 = 0;
/// something we can write to (character device, fifo, socket, ..)
pub const NODE_WRITABLE: i32 = 1;
/// non-writable thing (e.g., block device)
pub const NODE_OTHER: i32 = 2;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_id_empty_matches_default() {
        assert_eq!(FileID::empty(), FileID::default());
        assert_eq!(FileID::empty(), FileID { inode: 0, device_id: 0 });
    }

    #[test]
    fn path_type_default_is_unknown() {
        assert_eq!(PathType::default(), PathType::Unknown);
        assert_eq!(PathType::DeviceUnc as i32, 5);
    }

    #[test]
    fn node_constants_are_distinct() {
        assert_ne!(NODE_NORMAL, NODE_WRITABLE);
        assert_ne!(NODE_WRITABLE, NODE_OTHER);
        assert_ne!(NODE_NORMAL, NODE_OTHER);
    }
}
