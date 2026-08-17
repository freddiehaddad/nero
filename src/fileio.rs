//! Translated from `src/nvim/fileio.c` (tractable core only).

//!
//! `fileio.c` (~3600 lines) is the real file-reading (`readfile()`)
//! and encoding-detection (`check_for_bom`) engine. Almost everything
//! needs real buffered file I/O and buffer-line construction, neither
//! translated.
//!
//! Also translated: [`is_dev_fd_file`]/[`readfile_linenr`]/
//! [`write_lnum_adjust`] - three self-contained helpers needing no
//! file I/O. `is_dev_fd_file` rejects `/dev/fd/0`, `/1` and `/2`
//! (opening those can hang the editor) but only as a LONE digit, so
//! `/dev/fd/10` stays valid. `write_lnum_adjust` leaves the `0`
//! "nothing is missing an EOL" sentinel alone rather than shifting it
//! into a real line number.
//!
//! Translated: `get_fio_flags` (resolve the `FIO_*` conversion flags
//! for a given encoding name, via `mbyte.c`'s already-real
//! `enc_canon_props`; the `ENC_DBCS` branch needs `iconv()`, not
//! translated, but simply returns `0` in the original too - no
//! shortcut taken, this is the real behavior) and `ucs2bytes`
//! (`static`/private in the original - encode one Unicode codepoint
//! as bytes in a given `FIO_*` encoding; needed by `bufwrite.c`'s
//! `make_bom`).
//!
//! Also translated: the temp-directory family (`vim_mktempdir`,
//! `vim_settempdir`, `vim_gettempdir`, `vim_deltempdir`,
//! `vim_tempname`), the directory helpers it builds on
//! (`readdir_core`, `delete_recursive`), and `vim_copyfile`.
//!
//! Deferred: everything else in the file.

use crate::mbyte::enc_canon_props;

/// Shorten a buffer's displayed file name relative to `dirname`
/// (`shorten_buf_fname`).
///
/// # Safety
/// Must not run concurrently with option/global state read by the
/// path-comparison helpers.
pub unsafe fn shorten_buf_fname(
    buf: &mut crate::buffer_defs::BufT,
    dirname: &[u8],
    force: bool,
) {
    let should_shorten = buf.b_fname.as_deref().is_some_and(|name| {
        !crate::buffer::bt_nofilename(Some(buf))
            && crate::path::path_with_url(name) == 0
            && (force
                || buf.b_sfname.is_none()
                || buf
                    .b_sfname
                    .as_deref()
                    .is_some_and(crate::path::path_is_absolute))
    });
    if !should_shorten {
        return;
    }

    let shortened = buf.b_ffname.as_deref().and_then(|full| {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::path::path_shorten_fname(full, dirname) }.map(<[u8]>::to_vec)
    });
    if let Some(shortened) = shortened {
        buf.b_sfname = Some(shortened.clone());
        buf.b_fname = Some(shortened);
    } else {
        buf.b_sfname = None;
        buf.b_fname.clone_from(&buf.b_ffname);
    }
}

/// The path separator this platform's `PATHSEPSTR` appends.
const PATHSEP: u8 = if cfg!(windows) { b'\\' } else { b'/' };

/// Nvim's own temp directory, ending with a path separator
/// (`vim_tempdir`, a `static` in the original).
///
/// `None` until [`vim_mktempdir`] has successfully created one.
static VIM_TEMPDIR: std::sync::LazyLock<crate::globals::GlobalCell<Option<Vec<u8>>>> =
    std::sync::LazyLock::new(|| crate::globals::GlobalCell::new(None));

/// Counter behind [`vim_tempname`]'s unique names (`temp_count`, a
/// function-local `static` in the original).
static TEMP_COUNT: std::sync::LazyLock<crate::globals::GlobalCell<u64>> =
    std::sync::LazyLock::new(|| crate::globals::GlobalCell::new(0));

/// Append a line-ending format marker to `IObuff`
/// (`msg_add_fileformat`).
///
/// # Safety
/// Mutates `GLOBALS.IObuff`.
#[must_use]
pub unsafe fn msg_add_fileformat(eol_type: i32) -> bool {
    let suffix: Option<&[u8]> = if !cfg!(windows)
        && eol_type == crate::option_vars::EOL_DOS
    {
        Some(b"[dos]")
    } else if eol_type == crate::option_vars::EOL_MAC {
        Some(b"[mac]")
    } else if cfg!(windows) && eol_type == crate::option_vars::EOL_UNIX {
        Some(b"[unix]")
    } else {
        None
    };
    let Some(suffix) = suffix else {
        return false;
    };
    // SAFETY: forwarded from this function's own safety doc.
    let io = &mut unsafe { crate::globals::GLOBALS.get_mut() }.IObuff;
    crate::memory::xstrlcat(io, suffix, crate::globals::IOSIZE);
    true
}

/// Set Nvim's own temp directory to `tempdir`, which must already
/// exist (`vim_settempdir`).
///
/// Expands `tempdir` to a full path first, so a later `:cd` cannot
/// change what it refers to, and guarantees a trailing path separator.
///
/// The original returns `false` only when its `MAXPATHL + 2` scratch
/// allocation fails; a growing `Vec<u8>` cannot fail that way, so this
/// always succeeds and returns `bool` purely to keep the caller's own
/// structure intact.
///
/// # Safety
/// Mutates the shared `VIM_TEMPDIR` file-static.
unsafe fn vim_settempdir(tempdir: &[u8]) -> bool {
    // MSWIN passes force=true, every other platform false.
    let (mut buf, _ok) = crate::path::vim_full_name(tempdir, cfg!(windows));
    if !crate::path::after_pathsep(&buf, buf.len()) {
        buf.push(PATHSEP);
    }
    let cell = unsafe { VIM_TEMPDIR.get_mut() };
    *cell = Some(buf);
    true
}

/// Create Nvim's own temp directory (`vim_mktempdir`).
///
/// Tries each of `os_defs::TEMP_DIR_NAMES` until one works, creating
/// `<parent>/nvim.<user>/XXXXXX` (the `XXXXXX` replaced with random
/// characters by [`crate::os::fs::os_mkdtemp`]).
///
/// The `nvim.<user>/` level is skipped when it cannot be created as a
/// directory genuinely owned by this user with mode 0700, exactly as
/// upstream does - otherwise one user could deny service to another by
/// pre-creating `/tmp/nvim.<them>/`.
///
/// The original's `DLOG`/`WLOG`/`ELOG` diagnostics are omitted (the
/// message pipeline is not translated); every state change and every
/// control-flow decision is kept.
///
/// # Safety
/// Forwarded from `expand_env`/`vim_settempdir`'s own safety docs.
pub unsafe fn vim_mktempdir() {
    let mut user = match crate::os::users::os_get_username() {
        Ok(name) | Err(name) => name,
    };
    // Usernames may contain slashes (upstream #19240), which would
    // otherwise turn one directory level into several.
    crate::memory::memchrsub(&mut user, b'/', b'_');
    crate::memory::memchrsub(&mut user, b'\\', b'_');

    // Make sure the umask doesn't remove the executable bit; "repl"
    // has been reported to use 0177.
    #[cfg(unix)]
    // SAFETY: umask() has no preconditions and cannot fail; the saved
    // value is restored before returning, exactly as upstream does.
    let umask_save = unsafe { libc::umask(0o077) };

    for dir_name in crate::os::os_defs::TEMP_DIR_NAMES {
        // SAFETY: forwarded from this function's own safety doc.
        let mut tmp = unsafe { crate::os::env::expand_env(dir_name.as_bytes()) };
        if !crate::os::fs::os_isdir(&bytes_to_path(&tmp)) {
            // Upstream distinguishes "$TMPDIR unset" from "$TMPDIR set
            // but not a directory" purely to log a different message;
            // both simply move on to the next candidate.
            continue;
        }

        // "<parent>" exists, now try to create "<parent>/nvim.<user>/".
        if !crate::path::after_pathsep(&tmp, tmp.len()) {
            tmp.push(PATHSEP);
        }
        let without_user_len = tmp.len();
        tmp.extend_from_slice(b"nvim.");
        tmp.extend_from_slice(&user);

        let tmp_path = bytes_to_path(&tmp);
        // Always create, to avoid a race.
        crate::os::fs::os_mkdir(&tmp_path, 0o700);
        let owned = crate::os::fs::os_file_owned(&tmp_path);
        let isdir = crate::os::fs::os_isdir(&tmp_path);
        // XDG_RUNTIME_DIR must be owned by the user, mode 0700.
        #[cfg(unix)]
        let valid = {
            let perm = crate::os::fs::os_getperm(&tmp_path);
            isdir && owned && 0o700 == (perm & 0o777)
        };
        #[cfg(not(unix))]
        // Upstream's own `// TODO(justinmk): Windows ACL?` - no
        // permission component is checked off Unix.
        let valid = isdir && owned;

        if valid {
            if !crate::path::after_pathsep(&tmp, tmp.len()) {
                tmp.push(PATHSEP);
            }
        } else {
            // If our "root" tempdir is invalid or fails, proceed
            // without "<user>/" - else user1 could break user2 by
            // creating "/tmp/nvim.user2/".
            tmp.truncate(without_user_len);
        }

        // Now try to create "<parent>/nvim.<user>/XXXXXX". "XXXXXX" is
        // the mkdtemp template, replaced with random characters.
        tmp.extend_from_slice(b"XXXXXX");
        let Some(path) = crate::os::fs::os_mkdtemp(&tmp) else {
            continue;
        };

        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { vim_settempdir(&path) } {
            // Successfully created and set, so stop trying.
            break;
        }
        // Couldn't set vim_tempdir to path, so remove what we created.
        crate::os::fs::os_rmdir(&bytes_to_path(&path));
    }

    #[cfg(unix)]
    // SAFETY: restoring the value saved above; umask cannot fail.
    unsafe {
        libc::umask(umask_save);
    }
}

/// Delete Nvim's own temp directory and everything in it
/// (`vim_deltempdir`).
///
/// Does nothing when no tempdir was ever created.
///
/// The original's `HAVE_DIRFD_AND_FLOCK`-gated `vim_closetempdir()`
/// call is omitted along with its `vim_opentempdir()` counterpart:
/// that pair holds an open directory handle plus a `flock` purely to
/// stop a system tmp-cleaner from reaping the directory mid-session,
/// which needs the still-deferred `Directory`/`os_scandir` FFI. Its
/// absence costs only that protection, never correctness of the
/// deletion itself.
///
/// # Safety
/// Mutates the shared `VIM_TEMPDIR` file-static.
pub unsafe fn vim_deltempdir() {
    let cell = unsafe { VIM_TEMPDIR.get_mut() };
    let Some(dir) = cell.take() else {
        return;
    };
    // Remove the trailing path separator vim_settempdir added, so the
    // directory itself (not a child of it) is what gets deleted.
    let trimmed = &dir[..crate::path::path_tail(&dir).saturating_sub(1)];
    delete_recursive(trimmed);
}

/// Get the path to Nvim's own temp dir, ending with a path separator
/// (`vim_gettempdir`).
///
/// Creates the directory on the first call, and re-creates it if it
/// has since disappeared (an antivirus or an over-eager cleanup job
/// can genuinely delete it mid-session).
///
/// The original's `notfound` counter exists only to decide which
/// diagnostic to emit, so it is omitted along with those messages;
/// the re-creation behaviour itself is kept exactly.
///
/// # Safety
/// Forwarded from [`vim_mktempdir`]'s own safety doc.
pub unsafe fn vim_gettempdir() -> Option<Vec<u8>> {
    let missing = {
        let cell = unsafe { VIM_TEMPDIR.get_mut() };
        match cell.as_ref() {
            None => true,
            Some(dir) => !crate::os::fs::os_isdir(&bytes_to_path(dir)),
        }
    };
    if missing {
        {
            let cell = unsafe { VIM_TEMPDIR.get_mut() };
            *cell = None;
        }
        unsafe { vim_mktempdir() };
    }
    let cell = unsafe { VIM_TEMPDIR.get_mut() };
    cell.clone()
}

/// Return a unique name usable for a temp file (`vim_tempname`).
///
/// The file itself is NOT created. There is no need to check whether
/// it already exists: we own the directory and nobody else creates
/// files in it.
///
/// @return `None` if Nvim cannot create its own temp directory.
///
/// # Safety
/// Forwarded from [`vim_gettempdir`]'s own safety doc.
pub unsafe fn vim_tempname() -> Option<Vec<u8>> {
    let tempdir = unsafe { vim_gettempdir() }?;
    let count = unsafe { TEMP_COUNT.get_mut() };
    let mut name = tempdir;
    name.extend_from_slice(count.to_string().as_bytes());
    *count = count.wrapping_add(1);
    Some(name)
}

/// Interpret raw path bytes as a [`std::path::Path`].
///
/// Every path in this crate is carried as `Vec<u8>`, matching the
/// original's own `char *`, while `std::fs` wants a `Path`.
fn bytes_to_path(bytes: &[u8]) -> std::path::PathBuf {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        std::path::PathBuf::from(std::ffi::OsStr::from_bytes(bytes))
    }
    #[cfg(not(unix))]
    {
        std::path::PathBuf::from(String::from_utf8_lossy(bytes).into_owned())
    }
}

/// Per-entry filter for [`readdir_core`] (`CheckItem`, `fileio.h`).
///
/// Returns a negative value to stop the walk entirely, `0` to skip
/// just that entry, and any positive value to keep it. The original's
/// separate `void *context` parameter is unnecessary here: a Rust
/// closure captures whatever state it needs directly.
pub type CheckItem<'a> = dyn FnMut(&[u8]) -> i64 + 'a;

/// Move all lines from `frombuf` to `tobuf` (`move_lines`).
///
/// @return [`crate::vim_defs::OK`]/[`crate::vim_defs::FAIL`].
///
/// The copy runs first and the delete only follows if every append
/// succeeded, so a failure part-way through leaves the SOURCE intact
/// rather than losing lines from both buffers. If a delete then fails,
/// the original gives up too - its own comment notes that putting the
/// saved lines back "might fail again".
///
/// `ml_append`/`ml_delete` operate on `curbuf`, so it is swapped
/// around each phase and restored at the end, exactly as upstream
/// does.
///
/// # Safety
/// `frombuf` and `tobuf` must be valid, non-null pointers to live,
/// distinct `BufT`s for the whole call. Forwarded from
/// [`crate::memline::ml_append`]/[`crate::memline::ml_delete`]'s own
/// safety docs; also mutates `GLOBALS.curbuf`.
pub unsafe fn move_lines(
    frombuf: *mut crate::buffer_defs::BufT,
    tobuf: *mut crate::buffer_defs::BufT,
) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let tbuf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
    let mut retval = crate::vim_defs::OK;

    // Copy the lines in "frombuf" to "tobuf".
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = tobuf;
    // SAFETY: forwarded from this function's own safety doc.
    let from_count = unsafe { (*frombuf).b_ml.ml_line_count };
    for lnum in 1..=from_count {
        // SAFETY: forwarded from this function's own safety doc.
        let line = unsafe { crate::memline::ml_get_buf(&mut *frombuf, lnum) };
        // SAFETY: forwarded from this function's own safety doc.
        let len = unsafe { crate::memline::ml_get_buf_len(&mut *frombuf, lnum) };
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { crate::memline::ml_append(lnum - 1, &line[..len as usize], 0, false) }
            == crate::vim_defs::FAIL
        {
            retval = crate::vim_defs::FAIL;
            break;
        }
    }

    // Delete all the lines in "frombuf".
    if retval != crate::vim_defs::FAIL {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = frombuf;
        // SAFETY: forwarded from this function's own safety doc.
        let mut lnum = unsafe { (*frombuf).b_ml.ml_line_count };
        while lnum > 0 {
            // SAFETY: forwarded from this function's own safety doc.
            if unsafe { crate::memline::ml_delete(lnum) } == crate::vim_defs::FAIL {
                // Oops! We could try putting back the saved lines, but
                // that might fail again...
                retval = crate::vim_defs::FAIL;
                break;
            }
            lnum -= 1;
        }
    }

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = tbuf;
    retval
}

/// Copy the file `from` to `to` (`vim_copyfile`).
///
/// A symlink is copied AS a symlink - its target text is read and a
/// new link with the same target is created - rather than having its
/// contents copied. That branch is `HAVE_READLINK`-gated upstream, so
/// on a platform without it (Windows) a symlink falls through to the
/// plain byte copy below, exactly as upstream does.
///
/// @return [`crate::vim_defs::OK`]/[`crate::vim_defs::FAIL`].
///
/// The original's `os_get_acl`/`os_set_acl`/`os_free_acl` calls are
/// omitted because upstream's own implementations of all three are
/// unconditional no-ops (`os_get_acl` always returns NULL; the other
/// two return immediately for a NULL argument). Nothing observable is
/// lost. Its `errmsg` local is likewise dropped: it is declared, never
/// assigned, and so its `semsg` branch is unreachable upstream too.
///
/// Refuses to overwrite an existing `to`, via the same
/// `UV_FS_COPYFILE_EXCL` the original passes.
pub fn vim_copyfile(from: &[u8], to: &[u8]) -> i32 {
    let from_path = bytes_to_path(from);
    let to_path = bytes_to_path(to);

    #[cfg(unix)]
    {
        // HAVE_READLINK: copy a symlink as a symlink.
        if std::fs::symlink_metadata(&from_path).is_ok_and(|m| m.is_symlink()) {
            let ret = match std::fs::read_link(&from_path) {
                Ok(target) => std::os::unix::fs::symlink(target, &to_path).is_ok(),
                // A failed readlink leaves the original at ret = -1,
                // never attempting the symlink() call.
                Err(_) => false,
            };
            return if ret { crate::vim_defs::OK } else { crate::vim_defs::FAIL };
        }
    }

    if crate::os::fs::os_copy(&from_path, &to_path, crate::os::fs::copyfile::EXCL) != 0 {
        return crate::vim_defs::FAIL;
    }
    crate::vim_defs::OK
}

/// Retrieve the sorted list of entries in the directory `path`
/// (`readdir_core`, the core of the `readdir()` builtin).
///
/// `.` and `..` are always excluded. `checkitem`, when given, decides
/// per entry: a negative value stops the walk entirely, `0` skips just
/// that entry, and any positive value keeps it.
///
/// @return `None` on failure (the directory could not be opened),
///         matching the original's `FAIL`. The original's separate
///         `garray_T` out-parameter is replaced by the returned
///         `Vec`, this crate's established preference.
///
/// The original's `os_scandir`/`os_scandir_next`/`os_closedir` trio is
/// expressed directly as `std::fs::read_dir`, which already excludes
/// `.` and `..` on every platform, so the original's explicit filter
/// for them has nothing left to reject.
///
/// The original's `smsg(0, _(e_notopen), path)` on failure is omitted
/// (the message pipeline is not translated); the `FAIL` it accompanies
/// is kept.
pub fn readdir_core(
    path: &[u8],
    mut checkitem: Option<&mut CheckItem<'_>>,
) -> Option<Vec<Vec<u8>>> {
    let entries = std::fs::read_dir(bytes_to_path(path)).ok()?;

    let mut gap: Vec<Vec<u8>> = Vec::new();
    for entry in entries.flatten() {
        let name = os_string_to_bytes(&entry.file_name());
        let mut ignore = false;
        if let Some(check) = checkitem.as_mut() {
            let r = check(&name);
            if r < 0 {
                break;
            }
            if r == 0 {
                ignore = true;
            }
        }
        if !ignore {
            gap.push(name);
        }
    }

    if !gap.is_empty() {
        crate::strings::sort_strings(&mut gap);
    }
    Some(gap)
}

/// Delete `name` and everything in it, recursively
/// (`delete_recursive`).
///
/// @return `0` for success, `-1` if some file was not deleted.
///
/// A failure on one entry is remembered but does NOT stop the walk -
/// every remaining entry is still attempted, exactly as upstream does.
///
/// Anything that is not a *real* directory (a plain file, or a symlink
/// even when it points at a directory) is removed as a single entry
/// rather than descended into, so a symlink's target is never touched.
pub fn delete_recursive(name: &[u8]) -> i32 {
    let mut result = 0;

    if crate::os::fs::os_isrealdir(&bytes_to_path(name)) {
        if let Some(entries) = readdir_core(name, None) {
            for entry in entries {
                // The original builds each child path in the shared
                // NameBuff; a local Vec avoids that global entirely.
                let mut child = name.to_vec();
                child.push(b'/');
                child.extend_from_slice(&entry);
                if delete_recursive(&child) != 0 {
                    // Remember the failure but continue deleting any
                    // further entries.
                    result = -1;
                }
            }
            if crate::os::fs::os_rmdir(&bytes_to_path(name)) != 0 {
                result = -1;
            }
        } else {
            result = -1;
        }
    } else {
        // Delete symlink only.
        result = if crate::os::fs::os_remove(&bytes_to_path(name)) == 0 { 0 } else { -1 };
    }

    result
}

/// Render an [`std::ffi::OsStr`] back into the raw bytes this crate
/// carries paths as - the inverse of [`bytes_to_path`].
fn os_string_to_bytes(s: &std::ffi::OsStr) -> Vec<u8> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        s.as_bytes().to_vec()
    }
    #[cfg(not(unix))]
    {
        s.to_string_lossy().into_owned().into_bytes()
    }
}

/// `FIO_*` conversion flags (`fileio.h`).
pub mod fio {
    /// Convert Latin1.
    pub const FIO_LATIN1: i32 = 0x01;
    /// Convert UTF-8.
    pub const FIO_UTF8: i32 = 0x02;
    /// Convert UCS-2.
    pub const FIO_UCS2: i32 = 0x04;
    /// Convert UCS-4.
    pub const FIO_UCS4: i32 = 0x08;
    /// Convert UTF-16.
    pub const FIO_UTF16: i32 = 0x10;
    /// Little endian.
    pub const FIO_ENDIAN_L: i32 = 0x80;
    /// Skip encoding conversion.
    pub const FIO_NOCONVERT: i32 = 0x2000;
    /// Check for BOM at start of file.
    pub const FIO_UCSBOM: i32 = 0x4000;
    /// Allow all formats.
    pub const FIO_ALL: i32 = -1;
}

/// Whether `fname` names a `/dev/fd/N` file that is safe to open
/// (`is_dev_fd_file`).
///
/// Some shells on some systems pass these in place of a real file.
/// `/dev/fd/0`, `/dev/fd/1` and `/dev/fd/2` are deliberately REJECTED
/// because opening those can hang the editor, but only when the digit
/// is the last character, so `/dev/fd/10` is still accepted.
#[must_use]
pub fn is_dev_fd_file(fname: &[u8]) -> bool {
    const PREFIX: &[u8] = b"/dev/fd/";
    if !fname.starts_with(PREFIX) {
        return false;
    }
    let Some(&first_digit) = fname.get(PREFIX.len()) else {
        return false;
    };
    if !crate::ascii_defs::ascii_isdigit(i32::from(first_digit)) {
        return false;
    }
    // Everything after the first digit must be digits, to the end.
    let rest = &fname[PREFIX.len() + 1..];
    let after = crate::charset::skipdigits(rest);
    if rest.get(after).is_some_and(|&c| c != crate::ascii_defs::NUL) {
        return false;
    }

    // A single digit 0/1/2 is the unsafe case; more digits are fine.
    let has_more_digits =
        rest.first().is_some_and(|&c| c != crate::ascii_defs::NUL);
    has_more_digits || !matches!(first_digit, b'0' | b'1' | b'2')
}

/// Estimate the line number reached after reading more bytes
/// (`readfile_linenr`), for error messages that include one.
///
/// `linecnt` is the buffer's line count before the extra bytes were
/// read, and `more` is those bytes.
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer.
#[must_use]
pub unsafe fn readfile_linenr(
    linecnt: crate::pos_defs::LinenrT,
    more: &[u8],
) -> crate::pos_defs::LinenrT {
    // SAFETY: forwarded from this function's own safety doc.
    let line_count = unsafe { (*crate::globals::GLOBALS.get_mut().curbuf).b_ml.ml_line_count };
    let newlines = crate::pos_defs::LinenrT::try_from(
        more.iter().filter(|&&c| c == b'\n').count(),
    )
    .unwrap_or(crate::pos_defs::LinenrT::MAX);
    line_count - linecnt + 1 + newlines
}

/// Adjust the line marked as missing its end-of-line for the next
/// write (`write_lnum_adjust`), used when `do_filter()` deletes the
/// filter's input lines.
///
/// Does nothing when no line is missing an EOL, so the sentinel `0`
/// is never shifted into a real line number.
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer.
pub unsafe fn write_lnum_adjust(offset: crate::pos_defs::LinenrT) {
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &mut *crate::globals::GLOBALS.get_mut().curbuf };
    if curbuf.b_no_eol_lnum != 0 {
        curbuf.b_no_eol_lnum += offset;
    }
}

/// Return the `FIO_*` flags needed for the internal conversion if
/// `name` was unicode or latin1, otherwise `0`. If `name` is empty,
/// uses `'encoding'` (`get_fio_flags`).
///
/// # Safety
/// `crate::option_vars::OPTION_VARS` must be a valid, initialized
/// singleton (same requirement as every other `OPTION_VARS`-reading
/// function in this crate).
#[must_use]
pub unsafe fn get_fio_flags(name: &[u8]) -> i32 {
    let owned_enc;
    let name = if name.is_empty() {
        // SAFETY: forwarded from this function's own safety doc.
        owned_enc = unsafe { crate::option_vars::OPTION_VARS.get_mut() }
            .p_enc
            .clone()
            .unwrap_or_default();
        &owned_enc[..]
    } else {
        name
    };
    let prop = enc_canon_props(name);
    if prop & crate::mbyte_defs::enc::ENC_UNICODE != 0 {
        if prop & crate::mbyte_defs::enc::ENC_2BYTE != 0 {
            if prop & crate::mbyte_defs::enc::ENC_ENDIAN_L != 0 {
                return fio::FIO_UCS2 | fio::FIO_ENDIAN_L;
            }
            return fio::FIO_UCS2;
        }
        if prop & crate::mbyte_defs::enc::ENC_4BYTE != 0 {
            if prop & crate::mbyte_defs::enc::ENC_ENDIAN_L != 0 {
                return fio::FIO_UCS4 | fio::FIO_ENDIAN_L;
            }
            return fio::FIO_UCS4;
        }
        if prop & crate::mbyte_defs::enc::ENC_2WORD != 0 {
            if prop & crate::mbyte_defs::enc::ENC_ENDIAN_L != 0 {
                return fio::FIO_UTF16 | fio::FIO_ENDIAN_L;
            }
            return fio::FIO_UTF16;
        }
        return fio::FIO_UTF8;
    }
    if prop & crate::mbyte_defs::enc::ENC_LATIN1 != 0 {
        return fio::FIO_LATIN1;
    }
    // must be ENC_DBCS, requires iconv() - not translated, matching
    // the original's own real (not a placeholder) `return 0;` here.
    0
}

/// Whether `file_info`'s modification time differs from the recorded
/// `mtime`/`mtime_ns` (`time_differs`).
///
/// On Linux and Windows a one-second slack is allowed on the seconds
/// part: a FAT filesystem stores only five bits of seconds, and the
/// roundoff happens when the inode is flushed, so the value can shift
/// by a second on its own. Elsewhere the comparison is exact.
#[must_use]
pub fn time_differs(file_info: &crate::os::fs::FileInfoT, mtime: i64, mtime_ns: i64) -> bool {
    let secs = crate::os::fs::os_fileinfo_mtime(file_info);
    let nsec = crate::os::fs::os_fileinfo_mtime_ns(file_info);

    if cfg!(any(target_os = "linux", windows)) {
        nsec != mtime_ns || secs - mtime > 1 || mtime - secs > 1
    } else {
        nsec != mtime_ns || secs != mtime
    }
}

/// Record `file_info`'s modification time, size and mode on `buf`, so
/// a later check can notice the file changing underneath us
/// (`buf_store_file_info`).
pub fn buf_store_file_info(
    buf: &mut crate::buffer_defs::BufT,
    file_info: &crate::os::fs::FileInfoT,
) {
    buf.b_mtime = crate::os::fs::os_fileinfo_mtime(file_info);
    buf.b_mtime_ns = crate::os::fs::os_fileinfo_mtime_ns(file_info);
    buf.b_orig_size = crate::os::fs::os_fileinfo_size(file_info);
    buf.b_orig_mode = crate::os::fs::os_fileinfo_mode(file_info);
}

/// Detect a byte-order mark at the start of `p` (`check_for_bom`).
///
/// Returns `Some((encoding_name, bom_len))` when the leading bytes
/// are a BOM that is compatible with `flags` (the `FIO_*` set already
/// chosen for this read), otherwise `None`.
///
/// `flags` genuinely narrows the result rather than merely filtering:
/// the same `FF FE` prefix reports `"ucs-2le"`, `"utf-16le"` or
/// nothing at all depending on it, and `utf-16` is preferred over
/// `ucs-2` for a bare `FE FF` because it also handles ucs-2 text.
///
/// The original takes `int *lenp` as an out-parameter and returns the
/// name separately; the length rides along in the returned tuple
/// here. It also relies on `p[1]` being readable whenever `p[0]` is;
/// this checks the slice length explicitly instead.
#[must_use]
pub fn check_for_bom(p: &[u8], size: i32, flags: i32) -> Option<(&'static [u8], i32)> {
    if p.len() < 2 {
        return None;
    }

    if p[0] == 0xef
        && p[1] == 0xbb
        && size >= 3
        && p.len() >= 3
        && p[2] == 0xbf
        && (flags == fio::FIO_ALL || flags == fio::FIO_UTF8 || flags == 0)
    {
        return Some((b"utf-8", 3)); // EF BB BF
    }

    if p[0] == 0xff && p[1] == 0xfe {
        if size >= 4
            && p.len() >= 4
            && p[2] == 0
            && p[3] == 0
            && (flags == fio::FIO_ALL || flags == (fio::FIO_UCS4 | fio::FIO_ENDIAN_L))
        {
            return Some((b"ucs-4le", 4)); // FF FE 00 00
        }
        if flags == (fio::FIO_UCS2 | fio::FIO_ENDIAN_L) {
            return Some((b"ucs-2le", 2)); // FF FE
        }
        if flags == fio::FIO_ALL || flags == (fio::FIO_UTF16 | fio::FIO_ENDIAN_L) {
            // utf-16le is preferred, it also works for ucs-2le text.
            return Some((b"utf-16le", 2)); // FF FE
        }
        return None;
    }

    if p[0] == 0xfe
        && p[1] == 0xff
        && (flags == fio::FIO_ALL || flags == fio::FIO_UCS2 || flags == fio::FIO_UTF16)
    {
        // Default to utf-16, it works also for ucs-2 text.
        return Some(if flags == fio::FIO_UCS2 {
            (b"ucs-2", 2) // FE FF
        } else {
            (b"utf-16", 2) // FE FF
        });
    }

    if size >= 4
        && p.len() >= 4
        && p[0] == 0
        && p[1] == 0
        && p[2] == 0xfe
        && p[3] == 0xff
        && (flags == fio::FIO_ALL || flags == fio::FIO_UCS4)
    {
        return Some((b"ucs-4", 4)); // 00 00 FE FF
    }

    None
}

/// Convert a Unicode character to bytes, appending them to `out`
/// (`ucs2bytes`). Returns `true` for an error, `false` when it's OK -
/// the original's own in-out `char **pp` pointer is replaced by
/// appending to a growing `Vec<u8>`, matching this crate's own
/// established "no separate length-then-fill pass needed" convention
/// (e.g. `grow_string_tv`).
pub fn ucs2bytes(c: u32, out: &mut Vec<u8>, flags: i32) -> bool {
    let mut error = false;

    if flags & fio::FIO_UCS4 != 0 {
        if flags & fio::FIO_ENDIAN_L != 0 {
            out.push(c as u8);
            out.push((c >> 8) as u8);
            out.push((c >> 16) as u8);
            out.push((c >> 24) as u8);
        } else {
            out.push((c >> 24) as u8);
            out.push((c >> 16) as u8);
            out.push((c >> 8) as u8);
            out.push(c as u8);
        }
    } else if flags & (fio::FIO_UCS2 | fio::FIO_UTF16) != 0 {
        let mut c = c;
        if c >= 0x10000 {
            if flags & fio::FIO_UTF16 != 0 {
                // Make two words, ten bits of the character in each.
                // First word is 0xd800-0xdbff, second 0xdc00-0xdfff.
                c -= 0x10000;
                if c >= 0x100000 {
                    error = true;
                }
                let cc = ((c >> 10) & 0x3ff) + 0xd800;
                if flags & fio::FIO_ENDIAN_L != 0 {
                    out.push(cc as u8);
                    out.push((cc >> 8) as u8);
                } else {
                    out.push((cc >> 8) as u8);
                    out.push(cc as u8);
                }
                c = (c & 0x3ff) + 0xdc00;
            } else {
                error = true;
            }
        }
        if flags & fio::FIO_ENDIAN_L != 0 {
            out.push(c as u8);
            out.push((c >> 8) as u8);
        } else {
            out.push((c >> 8) as u8);
            out.push(c as u8);
        }
    } else {
        // Latin1
        if c >= 0x100 {
            error = true;
            out.push(0xBF);
        } else {
            out.push(c as u8);
        }
    }

    error
}

/// Builds a file name from `fname` plus the extension `ext`
/// (`modname`).
///
/// With no `fname`, the current directory is used instead. The result
/// always ends in `ext`, has a basename of at most `BASENAMELEN`
/// characters (truncated if needed), and differs from `fname`: if
/// appending the extension alone would not change it, basename
/// characters are replaced by `_`, and if the basename is already all
/// underscores the first becomes `v`.
///
/// Returns `None` only when there is no `fname` and the current
/// directory cannot be determined.
#[must_use]
pub fn modname(fname: Option<&[u8]>, ext: &[u8], prepend_dot: bool) -> Option<Vec<u8>> {
    let mut prepend_dot = prepend_dot;
    let mut retval = match fname {
        Some(f) if !f.is_empty() => f.to_vec(),
        _ => {
            let mut dir = crate::os::fs::os_dirname()?;
            if dir.is_empty() {
                return None;
            }
            crate::path::add_pathsep(&mut dir);
            // There is no file name to prepend a dot to.
            prepend_dot = false;
            dir
        }
    };

    // The basename starts after the last path separator.
    let base_start = match retval.iter().rposition(|&c| crate::path::vim_ispathsep(i32::from(c))) {
        Some(i) => i + 1,
        None => 0,
    };

    // Limit the basename to BASENAMELEN characters.
    let max_base = base_start + crate::os::os_defs::BASENAMELEN as usize;
    if retval.len() > max_base {
        retval.truncate(max_base);
    }

    retval.extend_from_slice(ext);

    if prepend_dot {
        let tail = crate::path::path_tail(&retval);
        if retval.get(tail) != Some(&b'.') {
            retval.insert(tail, b'.');
        }
    }

    // Appending the extension may not have changed anything, e.g.
    // when it was already there.
    if fname == Some(retval.as_slice()) {
        // Find a character that can be replaced by '_', scanning back
        // from the end of the basename.
        let base_end = retval.len() - ext.len();
        match retval[base_start..base_end].iter().rposition(|&c| c != b'_') {
            Some(off) => retval[base_start + off] = b'_',
            // The basename was all underscores already.
            None => retval[base_start] = b'v',
        }
    }

    Some(retval)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shorten_buf_fname_uses_relative_name_or_restores_full_name() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT {
            b_ffname: Some(b"/home/user/file.txt".to_vec()),
            b_sfname: Some(b"/home/user/file.txt".to_vec()),
            b_fname: Some(b"/home/user/file.txt".to_vec()),
            ..Default::default()
        };
        unsafe { shorten_buf_fname(&mut buf, b"/home/user", false) };
        assert_eq!(buf.b_sfname.as_deref(), Some(&b"file.txt"[..]));
        assert_eq!(buf.b_fname.as_deref(), Some(&b"file.txt"[..]));

        buf.b_sfname = Some(b"/home/user/file.txt".to_vec());
        buf.b_fname = Some(b"/home/user/file.txt".to_vec());
        unsafe { shorten_buf_fname(&mut buf, b"/other", false) };
        assert!(buf.b_sfname.is_none());
        assert_eq!(
            buf.b_fname.as_deref(),
            Some(&b"/home/user/file.txt"[..])
        );
    }

    #[test]
    fn msg_add_fileformat_appends_non_native_line_ending_markers() {
        let _lock = crate::globals::global_state_test_lock();
        let mut initial = [0; crate::globals::IOSIZE];
        initial[..5].copy_from_slice(b"file ");
        let _io = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.IObuff,
                initial,
            )
        };

        assert!(unsafe { msg_add_fileformat(crate::option_vars::EOL_MAC) });
        let io = &unsafe { crate::globals::GLOBALS.get_mut() }.IObuff;
        let end = io.iter().position(|&byte| byte == 0).unwrap();
        assert_eq!(&io[..end], b"file [mac]");

        let native = if cfg!(windows) {
            crate::option_vars::EOL_DOS
        } else {
            crate::option_vars::EOL_UNIX
        };
        assert!(!unsafe { msg_add_fileformat(native) });
    }
    use crate::globals::global_state_test_lock;

    // ---- modname ----

    #[test]
    fn modname_appends_the_extension() {
        assert_eq!(modname(Some(b"/tmp/notes"), b".swp", false), Some(b"/tmp/notes.swp".to_vec()));
    }

    /// With prepend_dot the basename is hidden, but only when it is
    /// not hidden already.
    #[test]
    fn modname_prepends_a_dot_only_when_needed() {
        assert_eq!(modname(Some(b"/tmp/notes"), b".swp", true), Some(b"/tmp/.notes.swp".to_vec()));
        assert_eq!(
            modname(Some(b".notes"), b".swp", true),
            Some(b".notes.swp".to_vec()),
            "already hidden, so no second dot"
        );
    }

    /// The result must differ from the input. When appending the
    /// extension alone would not change it, the LAST non-underscore
    /// character of the basename becomes an underscore.
    #[test]
    fn modname_forces_a_difference_when_the_name_is_unchanged() {
        // "notes.swp" + ".swp" would normally be "notes.swp.swp", so
        // pass an empty extension to hit the equality case directly.
        assert_eq!(
            modname(Some(b"/tmp/notes"), b"", false),
            Some(b"/tmp/note_".to_vec()),
            "the last basename character is replaced"
        );
    }

    /// An all-underscore basename has nothing left to replace, so the
    /// first character becomes 'v' instead.
    #[test]
    fn modname_uses_v_when_the_basename_is_all_underscores() {
        assert_eq!(modname(Some(b"/tmp/____"), b"", false), Some(b"/tmp/v___".to_vec()));
    }

    /// The replacement scans only the basename, never the directory.
    #[test]
    fn modname_does_not_touch_the_directory_part() {
        let got = modname(Some(b"/a_b/_"), b"", false).unwrap();
        assert_eq!(got, b"/a_b/v".to_vec(), "the '_' in the directory is left alone");
    }

    /// A basename longer than BASENAMELEN is truncated before the
    /// extension is appended.
    #[test]
    fn modname_truncates_an_overlong_basename() {
        let long = vec![b'a'; crate::os::os_defs::BASENAMELEN as usize + 20];
        let mut fname = b"/tmp/".to_vec();
        fname.extend_from_slice(&long);

        let got = modname(Some(&fname), b".swp", false).unwrap();

        let base = &got[b"/tmp/".len()..];
        assert_eq!(
            base.len(),
            crate::os::os_defs::BASENAMELEN as usize + b".swp".len(),
            "basename truncated to BASENAMELEN, then the extension added"
        );
        assert!(got.ends_with(b".swp"));
    }

    /// Without a file name the current directory is used, and there
    /// is then nothing to prepend a dot to.
    #[test]
    fn modname_without_a_name_uses_the_current_directory() {
        let got = modname(None, b".swp", true).expect("cwd must be available");
        assert!(got.ends_with(b".swp"));
        let tail = crate::path::path_tail(&got);
        assert_eq!(got[tail..], b".swp"[..], "no basename, so no leading dot was added");
    }

    #[test]
    fn modname_treats_an_empty_name_like_none() {
        let from_empty = modname(Some(b""), b".swp", false);
        let from_none = modname(None, b".swp", false);
        assert_eq!(from_empty, from_none);
    }


    #[test]
    fn is_dev_fd_file_accepts_multi_digit_descriptors() {
        assert!(is_dev_fd_file(b"/dev/fd/3"));
        assert!(is_dev_fd_file(b"/dev/fd/10"));
        assert!(is_dev_fd_file(b"/dev/fd/123"));
    }

    #[test]
    fn is_dev_fd_file_rejects_the_three_standard_descriptors() {
        // Opening these can hang the editor, so they are excluded...
        assert!(!is_dev_fd_file(b"/dev/fd/0"));
        assert!(!is_dev_fd_file(b"/dev/fd/1"));
        assert!(!is_dev_fd_file(b"/dev/fd/2"));
        // ...but only as a LONE digit: a longer number starting with
        // one of them is a different descriptor and stays valid.
        assert!(is_dev_fd_file(b"/dev/fd/01"));
        assert!(is_dev_fd_file(b"/dev/fd/20"));
    }

    #[test]
    fn is_dev_fd_file_rejects_anything_else() {
        assert!(!is_dev_fd_file(b"/dev/fd/"));
        assert!(!is_dev_fd_file(b"/dev/fd/x"));
        // Trailing non-digits disqualify it.
        assert!(!is_dev_fd_file(b"/dev/fd/3x"));
        assert!(!is_dev_fd_file(b"/dev/null"));
        assert!(!is_dev_fd_file(b""));
    }

    #[test]
    fn readfile_linenr_counts_newlines_in_the_extra_bytes() {
        let _lock = global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT::default();
        buf.b_ml.ml_line_count = 10;
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev = globals.curbuf;
        globals.curbuf = &mut buf as *mut crate::buffer_defs::BufT;

        // 10 - 8 + 1 = 3, plus two more newlines.
        assert_eq!(unsafe { readfile_linenr(8, b"a\nb\nc") }, 5);
        // With no newlines the estimate is just the base.
        assert_eq!(unsafe { readfile_linenr(8, b"abc") }, 3);
        assert_eq!(unsafe { readfile_linenr(10, b"") }, 1);

        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev;
    }

    #[test]
    fn write_lnum_adjust_only_shifts_a_real_line() {
        let _lock = global_state_test_lock();
        let mut buf = crate::buffer_defs::BufT::default();
        let ptr: *mut crate::buffer_defs::BufT = &mut buf;
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev = globals.curbuf;
        globals.curbuf = ptr;

        // Everything below goes through `ptr` rather than touching
        // `buf` directly: interleaving the two invalidates the raw
        // pointer's tag under Tree Borrows, which Miri rejects.
        //
        // 0 is the "no line is missing an EOL" sentinel, so it must
        // not be shifted into a real line number.
        unsafe { (*ptr).b_no_eol_lnum = 0 };
        unsafe { write_lnum_adjust(5) };
        assert_eq!(unsafe { (*ptr).b_no_eol_lnum }, 0);

        unsafe { (*ptr).b_no_eol_lnum = 7 };
        unsafe { write_lnum_adjust(5) };
        assert_eq!(unsafe { (*ptr).b_no_eol_lnum }, 12);
        // A negative offset shifts back the other way.
        unsafe { write_lnum_adjust(-2) };
        assert_eq!(unsafe { (*ptr).b_no_eol_lnum }, 10);

        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev;
    }

    #[test]
    fn get_fio_flags_utf8() {
        assert_eq!(unsafe { get_fio_flags(b"utf-8") }, fio::FIO_UTF8);
    }

    #[test]
    fn get_fio_flags_latin1() {
        assert_eq!(unsafe { get_fio_flags(b"latin1") }, fio::FIO_LATIN1);
    }

    #[test]
    fn get_fio_flags_ucs2_big_endian() {
        assert_eq!(unsafe { get_fio_flags(b"ucs-2") }, fio::FIO_UCS2);
    }

    #[test]
    fn get_fio_flags_ucs2_little_endian() {
        assert_eq!(
            unsafe { get_fio_flags(b"ucs-2le") },
            fio::FIO_UCS2 | fio::FIO_ENDIAN_L
        );
    }

    #[test]
    fn get_fio_flags_ucs4_big_endian() {
        assert_eq!(unsafe { get_fio_flags(b"ucs-4") }, fio::FIO_UCS4);
    }

    #[test]
    fn get_fio_flags_utf16_little_endian() {
        assert_eq!(
            unsafe { get_fio_flags(b"utf-16le") },
            fio::FIO_UTF16 | fio::FIO_ENDIAN_L
        );
    }

    #[test]
    fn get_fio_flags_dbcs_returns_zero() {
        assert_eq!(unsafe { get_fio_flags(b"sjis") }, 0);
    }

    #[test]
    fn get_fio_flags_unknown_name_returns_zero() {
        assert_eq!(unsafe { get_fio_flags(b"not-a-real-encoding") }, 0);
    }

    #[test]
    fn get_fio_flags_empty_name_uses_p_enc() {
        let _lock = global_state_test_lock();
        let saved = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_enc.clone();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_enc = Some(b"utf-8".to_vec());
        let result = unsafe { get_fio_flags(b"") };
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_enc = saved;
        assert_eq!(result, fio::FIO_UTF8);
    }

    // --- time_differs / buf_store_file_info ---

    /// A real file on disk, so the tests exercise genuine metadata
    /// rather than a hand-built struct.
    fn temp_file_info(name: &str) -> (std::path::PathBuf, crate::os::fs::FileInfoT) {
        let path = std::env::temp_dir().join(name);
        std::fs::write(&path, b"contents").unwrap();
        let info = crate::os::fs::os_fileinfo(&path).unwrap();
        (path, info)
    }

    #[test]
    fn time_differs_is_false_against_the_files_own_timestamp() {
        let (path, info) = temp_file_info("nero_test_time_differs_same");
        let secs = crate::os::fs::os_fileinfo_mtime(&info);
        let nsec = crate::os::fs::os_fileinfo_mtime_ns(&info);

        assert!(!time_differs(&info, secs, nsec));

        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn time_differs_notices_a_changed_nanosecond_part() {
        let (path, info) = temp_file_info("nero_test_time_differs_nsec");
        let secs = crate::os::fs::os_fileinfo_mtime(&info);
        let nsec = crate::os::fs::os_fileinfo_mtime_ns(&info);

        assert!(time_differs(&info, secs, nsec + 1));

        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn time_differs_notices_a_clearly_different_second() {
        let (path, info) = temp_file_info("nero_test_time_differs_secs");
        let secs = crate::os::fs::os_fileinfo_mtime(&info);
        let nsec = crate::os::fs::os_fileinfo_mtime_ns(&info);

        // Well outside the one-second FAT slack, so every platform
        // agrees this is a real change.
        assert!(time_differs(&info, secs + 100, nsec));
        assert!(time_differs(&info, secs - 100, nsec));

        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    #[cfg(any(target_os = "linux", windows))]
    fn time_differs_allows_one_second_of_fat_slack() {
        // Only the seconds part is slackened, so the nanosecond part
        // has to match for this to report "unchanged".
        let (path, info) = temp_file_info("nero_test_time_differs_slack");
        let secs = crate::os::fs::os_fileinfo_mtime(&info);
        let nsec = crate::os::fs::os_fileinfo_mtime_ns(&info);

        assert!(!time_differs(&info, secs + 1, nsec));
        assert!(!time_differs(&info, secs - 1, nsec));
        assert!(time_differs(&info, secs + 2, nsec));

        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn buf_store_file_info_records_mtime_size_and_mode() {
        let (path, info) = temp_file_info("nero_test_buf_store_file_info");
        let mut buf = crate::buffer_defs::BufT::default();

        buf_store_file_info(&mut buf, &info);

        assert_eq!(buf.b_mtime, crate::os::fs::os_fileinfo_mtime(&info));
        assert_eq!(buf.b_mtime_ns, crate::os::fs::os_fileinfo_mtime_ns(&info));
        assert_eq!(buf.b_orig_size, 8, "\"contents\" is 8 bytes");
        assert_eq!(buf.b_orig_mode, crate::os::fs::os_fileinfo_mode(&info));
        // What was just stored must compare as unchanged.
        assert!(!time_differs(&info, buf.b_mtime, buf.b_mtime_ns));

        std::fs::remove_file(&path).unwrap();
    }

    // --- check_for_bom ---

    #[test]
    fn check_for_bom_detects_a_utf8_bom() {
        // Cross-verified against real nvim: a file starting EF BB BF
        // is opened with 'fileencoding' utf-8 and 'bomb' set.
        let p = [0xEF, 0xBB, 0xBF, b'h', b'i'];
        assert_eq!(
            check_for_bom(&p, p.len() as i32, fio::FIO_ALL),
            Some((&b"utf-8"[..], 3))
        );
    }

    #[test]
    fn check_for_bom_prefers_utf16le_over_ucs2le_for_ff_fe() {
        // Cross-verified against real nvim: a FF FE file reports
        // 'fileencoding' utf-16le, not ucs-2le.
        let p = [0xFF, 0xFE, b'h', 0];
        assert_eq!(
            check_for_bom(&p, p.len() as i32, fio::FIO_ALL),
            Some((&b"utf-16le"[..], 2))
        );
    }

    #[test]
    fn check_for_bom_ff_fe_narrows_by_flags() {
        // The same prefix reports different encodings - the flags
        // genuinely select, they do not merely filter.
        let p = [0xFF, 0xFE, b'h', 0];
        assert_eq!(
            check_for_bom(&p, p.len() as i32, fio::FIO_UCS2 | fio::FIO_ENDIAN_L),
            Some((&b"ucs-2le"[..], 2))
        );
        assert_eq!(
            check_for_bom(&p, p.len() as i32, fio::FIO_UTF8),
            None,
            "incompatible flags reject the BOM entirely"
        );
    }

    #[test]
    fn check_for_bom_ff_fe_00_00_is_ucs4le_when_long_enough() {
        let p = [0xFF, 0xFE, 0x00, 0x00];
        assert_eq!(
            check_for_bom(&p, 4, fio::FIO_ALL),
            Some((&b"ucs-4le"[..], 4))
        );
        // With only two bytes available it falls back to utf-16le.
        assert_eq!(
            check_for_bom(&p, 2, fio::FIO_ALL),
            Some((&b"utf-16le"[..], 2))
        );
    }

    #[test]
    fn check_for_bom_detects_the_big_endian_forms() {
        // Cross-verified against real nvim: a FE FF file reports
        // 'fileencoding' utf-16.
        let p = [0xFE, 0xFF, 0, b'h'];
        assert_eq!(
            check_for_bom(&p, p.len() as i32, fio::FIO_ALL),
            Some((&b"utf-16"[..], 2))
        );
        assert_eq!(
            check_for_bom(&p, p.len() as i32, fio::FIO_UCS2),
            Some((&b"ucs-2"[..], 2))
        );

        let q = [0x00, 0x00, 0xFE, 0xFF];
        assert_eq!(check_for_bom(&q, 4, fio::FIO_ALL), Some((&b"ucs-4"[..], 4)));
    }

    #[test]
    fn check_for_bom_rejects_plain_text_and_short_input() {
        assert_eq!(check_for_bom(b"hello", 5, fio::FIO_ALL), None);
        // The original reads p[1] unconditionally; this checks the
        // slice length instead of relying on the caller.
        assert_eq!(check_for_bom(&[0xEF], 1, fio::FIO_ALL), None);
        assert_eq!(check_for_bom(b"", 0, fio::FIO_ALL), None);
    }

    #[test]
    fn ucs2bytes_utf8_is_not_handled_here_latin1_fallback_for_ascii() {
        let mut out = Vec::new();
        let error = ucs2bytes(0x41, &mut out, fio::FIO_LATIN1);
        assert!(!error);
        assert_eq!(out, vec![0x41]);
    }

    #[test]
    fn ucs2bytes_latin1_out_of_range_errors_and_writes_0xbf() {
        let mut out = Vec::new();
        let error = ucs2bytes(0x1000, &mut out, fio::FIO_LATIN1);
        assert!(error);
        assert_eq!(out, vec![0xBF]);
    }

    #[test]
    fn ucs2bytes_ucs2_big_endian() {
        let mut out = Vec::new();
        let error = ucs2bytes(0xfeff, &mut out, fio::FIO_UCS2);
        assert!(!error);
        assert_eq!(out, vec![0xfe, 0xff]);
    }

    #[test]
    fn ucs2bytes_ucs2_little_endian() {
        let mut out = Vec::new();
        let error = ucs2bytes(0xfeff, &mut out, fio::FIO_UCS2 | fio::FIO_ENDIAN_L);
        assert!(!error);
        assert_eq!(out, vec![0xff, 0xfe]);
    }

    #[test]
    fn ucs2bytes_ucs4_little_endian() {
        let mut out = Vec::new();
        let error = ucs2bytes(0x0001_0000, &mut out, fio::FIO_UCS4 | fio::FIO_ENDIAN_L);
        assert!(!error);
        assert_eq!(out, vec![0x00, 0x00, 0x01, 0x00]);
    }

    #[test]
    fn ucs2bytes_ucs4_big_endian() {
        let mut out = Vec::new();
        let error = ucs2bytes(0x0001_0000, &mut out, fio::FIO_UCS4);
        assert!(!error);
        assert_eq!(out, vec![0x00, 0x01, 0x00, 0x00]);
    }

    #[test]
    fn ucs2bytes_utf16_surrogate_pair_big_endian() {
        // U+10000 -> surrogate pair 0xD800 0xDC00 (the smallest
        // codepoint requiring UTF-16 surrogate encoding).
        let mut out = Vec::new();
        let error = ucs2bytes(0x0001_0000, &mut out, fio::FIO_UTF16);
        assert!(!error);
        assert_eq!(out, vec![0xd8, 0x00, 0xdc, 0x00]);
    }

    #[test]
    fn ucs2bytes_utf16_surrogate_pair_little_endian() {
        let mut out = Vec::new();
        let error = ucs2bytes(0x0001_0000, &mut out, fio::FIO_UTF16 | fio::FIO_ENDIAN_L);
        assert!(!error);
        assert_eq!(out, vec![0x00, 0xd8, 0x00, 0xdc]);
    }

    #[test]
    fn ucs2bytes_ucs2_out_of_range_errors_via_utf16_fallback_path() {
        // FIO_UCS2 (without FIO_UTF16) can't represent codepoints
        // >= 0x10000 at all - the original's own `else { error =
        // true; }` branch, still writing the low 16 bits as a
        // (wrong, but faithfully-replicated) 2-byte value.
        let mut out = Vec::new();
        let error = ucs2bytes(0x0001_0000, &mut out, fio::FIO_UCS2);
        assert!(error);
        assert_eq!(out, vec![0x00, 0x00]);
    }

    // --- vim_mktempdir / vim_gettempdir / vim_tempname ---
    //
    // These all mutate the shared VIM_TEMPDIR/TEMP_COUNT file-statics
    // and create real directories, so every one of them holds
    // global_state_test_lock() for its whole body. Each also resets
    // VIM_TEMPDIR afterwards so a later test cannot observe a stale
    // (or since-deleted) directory left behind by this one.

    /// Restores `VIM_TEMPDIR` to its pre-test value on drop, even if
    /// the test panics, and removes any directory this test created.
    struct TempdirGuard {
        saved: Option<Vec<u8>>,
    }

    impl TempdirGuard {
        fn new() -> Self {
            let saved = unsafe { VIM_TEMPDIR.get_mut() }.clone();
            unsafe { *VIM_TEMPDIR.get_mut() = None };
            TempdirGuard { saved }
        }
    }

    impl Drop for TempdirGuard {
        fn drop(&mut self) {
            let created = unsafe { VIM_TEMPDIR.get_mut() }.clone();
            if let Some(dir) = created {
                let _ = std::fs::remove_dir_all(bytes_to_path(&dir));
            }
            unsafe { *VIM_TEMPDIR.get_mut() = self.saved.take() };
        }
    }

    #[test]
    fn vim_gettempdir_creates_a_real_directory_ending_in_a_separator() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = TempdirGuard::new();

        let dir = unsafe { vim_gettempdir() }.expect("a tempdir must be creatable");
        assert!(!dir.is_empty());
        // The original guarantees a trailing path separator so callers
        // can concatenate a file name directly.
        assert_eq!(*dir.last().unwrap(), PATHSEP);
        assert!(crate::os::fs::os_isdir(&bytes_to_path(&dir)));
    }

    #[test]
    fn vim_gettempdir_is_stable_across_calls() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = TempdirGuard::new();

        let first = unsafe { vim_gettempdir() }.expect("a tempdir must be creatable");
        let second = unsafe { vim_gettempdir() }.expect("still creatable");
        // The second call must reuse the existing directory, not make
        // a fresh one.
        assert_eq!(first, second);
    }

    #[test]
    fn vim_gettempdir_recreates_a_disappeared_directory() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = TempdirGuard::new();

        let first = unsafe { vim_gettempdir() }.expect("a tempdir must be creatable");
        // An antivirus or over-eager cleanup job can genuinely delete
        // it mid-session; the original detects that and remakes it.
        std::fs::remove_dir_all(bytes_to_path(&first)).unwrap();
        assert!(!crate::os::fs::os_isdir(&bytes_to_path(&first)));

        let second = unsafe { vim_gettempdir() }.expect("must be recreated");
        assert_ne!(first, second);
        assert!(crate::os::fs::os_isdir(&bytes_to_path(&second)));
    }

    #[test]
    fn vim_mktempdir_creates_a_directory_under_a_temp_dir_name_candidate() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = TempdirGuard::new();

        unsafe { vim_mktempdir() };
        let dir = unsafe { VIM_TEMPDIR.get_mut() }
            .clone()
            .expect("vim_mktempdir must set VIM_TEMPDIR");
        assert!(crate::os::fs::os_isdir(&bytes_to_path(&dir)));
        // The mkdtemp template's six random characters are always
        // present, so the leaf name is never the bare "nvim.<user>".
        assert!(dir.len() > b"nvim.".len() + 6);
    }

    #[test]
    fn vim_tempname_is_inside_the_tempdir_and_unique_per_call() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = TempdirGuard::new();

        let dir = unsafe { vim_gettempdir() }.expect("a tempdir must be creatable");
        let first = unsafe { vim_tempname() }.expect("a name must be producible");
        let second = unsafe { vim_tempname() }.expect("a name must be producible");

        assert!(first.starts_with(&dir));
        assert!(second.starts_with(&dir));
        assert_ne!(first, second);
        // The file itself is deliberately NOT created.
        assert!(!bytes_to_path(&first).exists());
    }

    // --- readdir_core / delete_recursive ---

    /// Builds a small real directory tree under the system temp dir
    /// and removes whatever survives on drop.
    struct TreeScratch {
        root: std::path::PathBuf,
    }

    impl TreeScratch {
        fn new(name: &str) -> Self {
            static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let unique = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let mut root = std::env::temp_dir();
            root.push(format!("nero_fileio_test_{name}_{}_{unique}", std::process::id()));
            std::fs::create_dir_all(&root).unwrap();
            TreeScratch { root }
        }

        fn bytes(&self) -> Vec<u8> {
            os_string_to_bytes(self.root.as_os_str())
        }

        fn bytes_joined(&self, child: &[u8]) -> Vec<u8> {
            let mut p = self.bytes();
            p.push(b'/');
            p.extend_from_slice(child);
            p
        }
    }

    impl Drop for TreeScratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn readdir_core_lists_entries_sorted_and_without_dot_entries() {
        let scratch = TreeScratch::new("readdir_sorted");
        std::fs::write(scratch.root.join("b.txt"), b"b").unwrap();
        std::fs::write(scratch.root.join("a.txt"), b"a").unwrap();
        std::fs::create_dir(scratch.root.join("c_dir")).unwrap();

        let entries = readdir_core(&scratch.bytes(), None).expect("a real directory");
        // The original sorts, and never reports "." or "..".
        assert_eq!(entries, vec![b"a.txt".to_vec(), b"b.txt".to_vec(), b"c_dir".to_vec()]);
    }

    #[test]
    fn readdir_core_is_none_for_a_path_that_is_not_a_directory() {
        let scratch = TreeScratch::new("readdir_missing");
        assert_eq!(readdir_core(&scratch.bytes_joined(b"nope"), None), None);
    }

    #[test]
    fn readdir_core_checkitem_zero_skips_just_that_entry() {
        let scratch = TreeScratch::new("readdir_skip");
        std::fs::write(scratch.root.join("keep.txt"), b"k").unwrap();
        std::fs::write(scratch.root.join("skip.txt"), b"s").unwrap();

        let mut check = |name: &[u8]| i64::from(name != b"skip.txt");
        let entries = readdir_core(&scratch.bytes(), Some(&mut check)).expect("a real directory");
        assert_eq!(entries, vec![b"keep.txt".to_vec()]);
    }

    #[test]
    fn readdir_core_checkitem_negative_stops_the_walk() {
        let scratch = TreeScratch::new("readdir_stop");
        for n in ["a.txt", "b.txt", "c.txt"] {
            std::fs::write(scratch.root.join(n), b"x").unwrap();
        }
        // Stop as soon as anything is seen, so nothing is collected.
        let mut check = |_: &[u8]| -1_i64;
        let entries = readdir_core(&scratch.bytes(), Some(&mut check)).expect("a real directory");
        assert!(entries.is_empty());
    }

    #[test]
    fn delete_recursive_removes_a_whole_tree() {
        let scratch = TreeScratch::new("del_tree");
        let nested = scratch.root.join("a").join("b");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("deep.txt"), b"deep").unwrap();
        std::fs::write(scratch.root.join("top.txt"), b"top").unwrap();

        assert_eq!(delete_recursive(&scratch.bytes()), 0);
        assert!(!scratch.root.exists());
    }

    #[test]
    fn delete_recursive_removes_a_single_file() {
        let scratch = TreeScratch::new("del_file");
        let path = scratch.root.join("f.txt");
        std::fs::write(&path, b"x").unwrap();
        assert_eq!(delete_recursive(&os_string_to_bytes(path.as_os_str())), 0);
        assert!(!path.exists());
    }

    #[test]
    fn delete_recursive_reports_failure_for_a_missing_path() {
        let scratch = TreeScratch::new("del_missing");
        let missing = scratch.root.join("does_not_exist");
        assert_eq!(delete_recursive(&os_string_to_bytes(missing.as_os_str())), -1);
    }

    #[test]
    fn move_lines_transfers_every_line_and_empties_the_source() {
        // The copy runs first and the delete only follows if every
        // append succeeded, so the source is never emptied on a
        // partial failure.
        let _lock = crate::globals::global_state_test_lock();
        let prev_buf = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;

        let mut from = crate::buffer_defs::BufT::default();
        let mut to = crate::buffer_defs::BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut from) }, crate::vim_defs::OK);
        assert_eq!(unsafe { crate::memline::ml_open(&mut to) }, crate::vim_defs::OK);

        // Give the source three real lines.
        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = &mut from;
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(&mut from, 1, b"one\0") },
            crate::vim_defs::OK
        );
        assert_eq!(
            unsafe { crate::memline::ml_append(1, b"two", 0, false) },
            crate::vim_defs::OK
        );
        assert_eq!(
            unsafe { crate::memline::ml_append(2, b"three", 0, false) },
            crate::vim_defs::OK
        );
        assert_eq!(from.b_ml.ml_line_count, 3);

        assert_eq!(unsafe { move_lines(&mut from, &mut to) }, crate::vim_defs::OK);

        // Every line arrived, and the source is emptied down to the
        // single line an empty buffer always has.
        assert_eq!(from.b_ml.ml_line_count, 1);
        assert!(to.b_ml.ml_line_count >= 3);

        // curbuf is restored to whatever it was AT THE TIME OF THE
        // CALL - which the setup above left pointing at the source.
        assert_eq!(
            unsafe { crate::globals::GLOBALS.get_mut() }.curbuf,
            &raw mut from
        );

        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev_buf;
    }

    // --- vim_copyfile ---

    #[test]
    fn vim_copyfile_copies_a_plain_file() {
        let scratch = TreeScratch::new("copy_plain");
        let from = scratch.root.join("from.txt");
        let to = scratch.root.join("to.txt");
        std::fs::write(&from, b"payload").unwrap();

        let r = vim_copyfile(
            &os_string_to_bytes(from.as_os_str()),
            &os_string_to_bytes(to.as_os_str()),
        );
        assert_eq!(r, crate::vim_defs::OK);
        assert_eq!(std::fs::read(&to).unwrap(), b"payload");
        // The source must survive - copy, not move.
        assert!(from.exists());
    }

    #[test]
    fn vim_copyfile_refuses_to_overwrite_an_existing_destination() {
        // The original passes UV_FS_COPYFILE_EXCL, so an existing
        // destination is an error rather than being clobbered.
        let scratch = TreeScratch::new("copy_excl");
        let from = scratch.root.join("from.txt");
        let to = scratch.root.join("to.txt");
        std::fs::write(&from, b"new").unwrap();
        std::fs::write(&to, b"original").unwrap();

        let r = vim_copyfile(
            &os_string_to_bytes(from.as_os_str()),
            &os_string_to_bytes(to.as_os_str()),
        );
        assert_eq!(r, crate::vim_defs::FAIL);
        assert_eq!(std::fs::read(&to).unwrap(), b"original");
    }

    #[test]
    fn vim_copyfile_fails_for_a_missing_source() {
        let scratch = TreeScratch::new("copy_missing");
        let r = vim_copyfile(
            &scratch.bytes_joined(b"nope.txt"),
            &scratch.bytes_joined(b"to.txt"),
        );
        assert_eq!(r, crate::vim_defs::FAIL);
    }

    #[cfg(unix)]
    #[test]
    fn vim_copyfile_copies_a_symlink_as_a_symlink() {
        // HAVE_READLINK-gated upstream: the link's TARGET TEXT is
        // reproduced, rather than the target's contents being copied.
        let scratch = TreeScratch::new("copy_symlink");
        let target = scratch.root.join("target.txt");
        let link = scratch.root.join("link");
        let copy = scratch.root.join("copy");
        std::fs::write(&target, b"payload").unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();

        let r = vim_copyfile(
            &os_string_to_bytes(link.as_os_str()),
            &os_string_to_bytes(copy.as_os_str()),
        );
        assert_eq!(r, crate::vim_defs::OK);
        assert!(std::fs::symlink_metadata(&copy).unwrap().is_symlink());
        assert_eq!(std::fs::read_link(&copy).unwrap(), target);
    }

    #[test]
    fn vim_deltempdir_removes_the_tempdir_and_clears_the_static() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = TempdirGuard::new();

        let dir = unsafe { vim_gettempdir() }.expect("a tempdir must be creatable");
        assert!(crate::os::fs::os_isdir(&bytes_to_path(&dir)));

        unsafe { vim_deltempdir() };
        assert!(!crate::os::fs::os_isdir(&bytes_to_path(&dir)));
        // The static must be cleared, so a later vim_gettempdir makes
        // a fresh directory rather than handing back the dead one.
        assert!(unsafe { VIM_TEMPDIR.get_mut() }.is_none());
    }

    #[test]
    fn vim_deltempdir_without_a_tempdir_is_a_no_op() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = TempdirGuard::new();
        // TempdirGuard::new() already left VIM_TEMPDIR as None.
        unsafe { vim_deltempdir() };
        assert!(unsafe { VIM_TEMPDIR.get_mut() }.is_none());
    }
}
