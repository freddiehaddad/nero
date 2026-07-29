//! Translated from `src/nvim/bufwrite.c` (tractable core only).
//!
//! `bufwrite.c` (~1500 lines) is the real file-writing engine
//! (`buf_write`): backup-file creation, encoding conversion,
//! `BufWritePre`/`BufWritePost` autocmd triggers, and the actual
//! `os_write` calls. Almost everything needs real file I/O plus
//! `FileInfo`/backup-directory-search machinery, none translated.
//!
//! Translated: `make_bom` (build the byte-order-mark for a given
//! encoding name, via `fileio.c`'s already-real `get_fio_flags`/
//! `ucs2bytes`).
//!
//! Deferred: everything else in the file.

/// Generate the byte-order mark for encoding `name` (`make_bom`).
/// Returns an empty `Vec` when no BOM applies (Latin1, or an
/// unrecognized/DBCS encoding).
///
/// # Safety
/// Same as [`crate::fileio::get_fio_flags`].
#[must_use]
pub unsafe fn make_bom(name: &[u8]) -> Vec<u8> {
    // SAFETY: forwarded from this function's own safety doc.
    let flags = unsafe { crate::fileio::get_fio_flags(name) };

    // Can't put a BOM in a non-Unicode file.
    if flags == crate::fileio::fio::FIO_LATIN1 || flags == 0 {
        return Vec::new();
    }

    if flags == crate::fileio::fio::FIO_UTF8 {
        return vec![0xef, 0xbb, 0xbf];
    }

    let mut out = Vec::new();
    crate::fileio::ucs2bytes(0xfeff, &mut out, flags);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::globals::global_state_test_lock;

    #[test]
    fn make_bom_utf8() {
        assert_eq!(unsafe { make_bom(b"utf-8") }, vec![0xef, 0xbb, 0xbf]);
    }

    #[test]
    fn make_bom_latin1_is_empty() {
        assert_eq!(unsafe { make_bom(b"latin1") }, Vec::<u8>::new());
    }

    #[test]
    fn make_bom_unrecognized_name_is_empty() {
        assert_eq!(unsafe { make_bom(b"not-a-real-encoding") }, Vec::<u8>::new());
    }

    #[test]
    fn make_bom_dbcs_is_empty() {
        assert_eq!(unsafe { make_bom(b"sjis") }, Vec::<u8>::new());
    }

    #[test]
    fn make_bom_ucs2_big_endian() {
        assert_eq!(unsafe { make_bom(b"ucs-2") }, vec![0xfe, 0xff]);
    }

    #[test]
    fn make_bom_ucs2_little_endian() {
        assert_eq!(unsafe { make_bom(b"ucs-2le") }, vec![0xff, 0xfe]);
    }

    #[test]
    fn make_bom_ucs4_big_endian() {
        assert_eq!(unsafe { make_bom(b"ucs-4") }, vec![0x00, 0x00, 0xfe, 0xff]);
    }

    #[test]
    fn make_bom_utf16_big_endian() {
        assert_eq!(unsafe { make_bom(b"utf-16") }, vec![0xfe, 0xff]);
    }

    #[test]
    fn make_bom_empty_name_uses_p_enc() {
        let _lock = global_state_test_lock();
        let saved = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_enc.clone();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_enc = Some(b"utf-8".to_vec());
        let result = unsafe { make_bom(b"") };
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_enc = saved;
        assert_eq!(result, vec![0xef, 0xbb, 0xbf]);
    }
}
