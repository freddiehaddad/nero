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
//! `ucs2bytes`), plus the `Error_T` carrier and its `set_err*`
//! constructors.
//!
//! Deferred: everything else in the file.

/// A deferred write error, reported later by `emit_err` (`Error_T`).
///
/// `buf_write` collects a failure into one of these and emits it at a
/// single exit point, rather than reporting from deep inside the write
/// path.
///
/// The original's `bool alloc` field tracks whether `msg` needs
/// freeing; an owned `Vec<u8>` makes that unnecessary, so it has no
/// equivalent here.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ErrorT {
    /// Error number, e.g. `"E502"` (`num`). `None` for messages that
    /// carry no number of their own.
    pub num: Option<&'static [u8]>,
    /// The message itself (`msg`).
    pub msg: Option<Vec<u8>>,
    /// A single numeric argument interpolated into `msg` (`arg`).
    /// Zero means "no argument", matching the original.
    pub arg: i32,
}

/// A numbered error with no argument (`set_err_num`).
#[must_use]
pub fn set_err_num(num: &'static [u8], msg: &[u8]) -> ErrorT {
    ErrorT { num: Some(num), msg: Some(msg.to_vec()), arg: 0 }
}

/// An unnumbered error with no argument (`set_err`).
#[must_use]
pub fn set_err(msg: &[u8]) -> ErrorT {
    ErrorT { num: None, msg: Some(msg.to_vec()), arg: 0 }
}

/// An unnumbered error carrying a numeric argument (`set_err_arg`).
#[must_use]
pub fn set_err_arg(msg: &[u8], arg: i32) -> ErrorT {
    ErrorT { num: None, msg: Some(msg.to_vec()), arg }
}

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
    fn set_err_num_carries_a_number_and_no_argument() {
        let e = set_err_num(b"E502", b"is a directory");
        assert_eq!(e.num, Some(&b"E502"[..]));
        assert_eq!(e.msg.as_deref(), Some(&b"is a directory"[..]));
        assert_eq!(e.arg, 0);
    }

    #[test]
    fn set_err_carries_no_number_and_no_argument() {
        let e = set_err(b"is not a file");
        assert_eq!(e.num, None);
        assert_eq!(e.msg.as_deref(), Some(&b"is not a file"[..]));
        assert_eq!(e.arg, 0);
    }

    #[test]
    fn set_err_arg_carries_an_argument_but_no_number() {
        let e = set_err_arg(b"conversion failed in line %ld", 42);
        assert_eq!(e.num, None);
        assert_eq!(e.msg.as_deref(), Some(&b"conversion failed in line %ld"[..]));
        assert_eq!(e.arg, 42);
    }

    #[test]
    fn a_default_error_carries_nothing_at_all() {
        // Zero is the original's own "no argument" sentinel, so the
        // default must not look like a real argument.
        let e = ErrorT::default();
        assert_eq!(e.num, None);
        assert_eq!(e.msg, None);
        assert_eq!(e.arg, 0);
    }

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
