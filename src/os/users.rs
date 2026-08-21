//! Translated from `src/nvim/os/users.c` in full, except the original's
//! `EXITFREE`-only cache teardown (Rust's process-lifetime `LazyLock`
//! owns the cache).
//!
//! Translated: `os_get_username`, `os_get_uname`, `os_get_userdir`,
//! `os_get_usernames`, `init_users`/`get_users`, and `match_user`.

#[cfg(unix)]
static USER_DB_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(unix)]
fn user_db_lock() -> std::sync::MutexGuard<'static, ()> {
    USER_DB_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

static USERS: std::sync::LazyLock<Vec<Vec<u8>>> =
    std::sync::LazyLock::new(os_get_usernames);

/// Append one nonempty username (`add_user`).
fn add_user(users: &mut Vec<Vec<u8>>, user: Option<&[u8]>) {
    if let Some(user) = user
        && !user.is_empty()
    {
        users.push(user.to_vec());
    }
}

/// Gets the username associated with `uid` (`os_get_uname`).
///
/// @return `Ok(name)` if a real username was found, `Err(fallback)`
///         holding the stringified `uid` otherwise - matches the
///         original's "return `FAIL`, but still fill the output buffer
///         with something useful (the numeric uid)" contract in a
///         single value instead of a separate out-buffer plus status
///         code.
pub fn os_get_uname(uid: u32) -> Result<Vec<u8>, Vec<u8>> {
    #[cfg(unix)]
    {
        let _lock = user_db_lock();
        // SAFETY: getpwuid is documented to return either a valid
        // pointer to a (non-reentrant, statically-owned) passwd
        // struct or NULL; the returned pointer, if non-null, is only
        // read here, never freed - matches the original's own use of
        // this same non-reentrant API (also never freed there).
        let pw = unsafe { libc::getpwuid(uid) };
        if !pw.is_null() {
            // SAFETY: pw is non-null per the check above; pw_name is
            // documented to be a valid NUL-terminated C string
            // whenever pw itself is valid.
            let name = unsafe { (*pw).pw_name };
            if !name.is_null() {
                // SAFETY: name is a valid NUL-terminated C string, see above.
                let bytes = unsafe { std::ffi::CStr::from_ptr(name) }.to_bytes();
                if !bytes.is_empty() {
                    return Ok(bytes.to_vec());
                }
            }
        }
    }
    #[cfg(not(unix))]
    {
        // No HAVE_PWD_FUNCS equivalent on this platform - always falls
        // through to the numeric fallback below, matching the
        // original's own `#ifdef HAVE_PWD_FUNCS`-gated (Unix-only)
        // lookup.
    }
    Err(uid.to_string().into_bytes())
}

/// Gets the username that owns the current Nvim process
/// (`os_get_username`).
///
/// @return `Ok(name)`/`Err(fallback)`, see [`os_get_uname`].
pub fn os_get_username() -> Result<Vec<u8>, Vec<u8>> {
    #[cfg(unix)]
    {
        // SAFETY: getuid() has no preconditions and cannot fail.
        let uid = unsafe { libc::getuid() };
        os_get_uname(uid)
    }
    #[cfg(not(unix))]
    {
        // The original's own comment: "TODO(equalsraf): Windows
        // GetUserName()" - real Windows GetUserName() is NOT called
        // here upstream; os_get_username hard-codes uid 0 on this
        // platform (HAVE_PWD_FUNCS is Unix-only), so this always
        // reports the numeric fallback ("0") - a known upstream
        // limitation, faithfully preserved rather than "fixed" by
        // this translation.
        os_get_uname(0)
    }
}

/// Enumerate all system usernames (`os_get_usernames`).
#[must_use]
pub fn os_get_usernames() -> Vec<Vec<u8>> {
    #[cfg(unix)]
    {
        let _lock = user_db_lock();
        let mut users = Vec::new();
        unsafe { libc::setpwent() };
        loop {
            let passwd = unsafe { libc::getpwent() };
            if passwd.is_null() {
                break;
            }
            let name = unsafe { (*passwd).pw_name };
            if name.is_null() {
                continue;
            }
            let name = unsafe { std::ffi::CStr::from_ptr(name) }.to_bytes();
            add_user(&mut users, Some(name));
        }
        unsafe { libc::endpwent() };

        if let Some(user) = crate::os::env::os_getenv(b"USER")
            && !user.is_empty()
            && !users.iter().any(|name| name == &user)
            && let Ok(cuser) = std::ffi::CString::new(user.as_slice())
        {
            let passwd = unsafe { libc::getpwnam(cuser.as_ptr()) };
            if !passwd.is_null() {
                let name = unsafe { (*passwd).pw_name };
                if !name.is_null() {
                    let name =
                        unsafe { std::ffi::CStr::from_ptr(name) }.to_bytes();
                    add_user(&mut users, Some(name));
                }
            }
        }
        users
    }

    #[cfg(windows)]
    {
        #[repr(C)]
        struct UserInfo0 {
            name: *mut u16,
        }

        #[link(name = "netapi32")]
        unsafe extern "system" {
            fn NetUserEnum(
                server_name: *const u16,
                level: u32,
                filter: u32,
                buffer: *mut *mut u8,
                preferred_max_len: u32,
                entries_read: *mut u32,
                total_entries: *mut u32,
                resume_handle: *mut u32,
            ) -> u32;
            fn NetApiBufferFree(buffer: *mut std::ffi::c_void) -> u32;
        }

        let mut buffer = std::ptr::null_mut();
        let mut entries_read = 0u32;
        let mut total_entries = 0u32;
        let status = unsafe {
            NetUserEnum(
                std::ptr::null(),
                0,
                0,
                &mut buffer,
                u32::MAX,
                &mut entries_read,
                &mut total_entries,
                std::ptr::null_mut(),
            )
        };
        let mut users = Vec::new();
        if status == 0 && !buffer.is_null() {
            let entries = unsafe {
                std::slice::from_raw_parts(
                    buffer.cast::<UserInfo0>(),
                    entries_read as usize,
                )
            };
            for entry in entries {
                if entry.name.is_null() {
                    continue;
                }
                let mut len = 0usize;
                while unsafe { *entry.name.add(len) } != 0 {
                    len += 1;
                }
                let name =
                    unsafe { std::slice::from_raw_parts(entry.name, len) };
                let name = String::from_utf16_lossy(name).into_bytes();
                add_user(&mut users, Some(&name));
            }
            unsafe { NetApiBufferFree(buffer.cast()) };
        }
        users
    }
}

/// Return cached username `idx` for shell completion (`get_users`).
#[must_use]
pub fn get_users(idx: i32) -> Option<&'static [u8]> {
    usize::try_from(idx)
        .ok()
        .and_then(|idx| USERS.get(idx))
        .map(Vec::as_slice)
}

fn match_user_in(users: &[Vec<u8>], name: &[u8]) -> i32 {
    let mut result = 0;
    for user in users {
        if user == name {
            return 2;
        }
        if user.starts_with(name) {
            result = 1;
        }
    }
    result
}

/// Match `name` against cached system usernames (`match_user`).
#[must_use]
pub fn match_user(name: &[u8]) -> i32 {
    match_user_in(&USERS, name)
}

/// Gets the home directory of the user named `name`, or `None`
/// (`os_get_userdir`).
///
/// The mirror image of [`os_get_uname`]'s uid -> passwd lookup: a name
/// -> passwd lookup, reporting the entry's `pw_dir` instead of its
/// `pw_name`.
///
/// Only meaningful where the original's own `HAVE_PWD_FUNCS` is
/// defined (Unix); every other platform always reports `None`, exactly
/// as the original does when that macro is absent.
pub fn os_get_userdir(name: &[u8]) -> Option<Vec<u8>> {
    if name.is_empty() {
        return None;
    }
    #[cfg(unix)]
    {
        let _lock = user_db_lock();
        // getpwnam takes a NUL-terminated C string; an interior NUL
        // would silently truncate the lookup, so reject it outright
        // rather than looking up a different user than asked for.
        let cname = std::ffi::CString::new(name).ok()?;
        // SAFETY: cname is a valid NUL-terminated C string alive for
        // this call. getpwnam returns either NULL or a pointer to a
        // non-reentrant, statically-owned passwd struct, which is only
        // read here and never freed - matching the original's own use
        // of this same API (also never freed there).
        let pw = unsafe { libc::getpwnam(cname.as_ptr()) };
        if !pw.is_null() {
            // SAFETY: pw is non-null per the check above; pw_dir is
            // documented to be a valid NUL-terminated C string
            // whenever pw itself is valid.
            let dir = unsafe { (*pw).pw_dir };
            if !dir.is_null() {
                // SAFETY: dir is a valid NUL-terminated C string, see above.
                let bytes = unsafe { std::ffi::CStr::from_ptr(dir) }.to_bytes();
                return Some(bytes.to_vec());
            }
        }
        None
    }
    #[cfg(not(unix))]
    {
        // No HAVE_PWD_FUNCS equivalent on this platform, so the
        // original's own lookup is compiled out entirely and it always
        // returns NULL here.
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_user_ignores_missing_and_empty_names_and_copies_values() {
        let mut users = vec![b"existing".to_vec()];
        add_user(&mut users, None);
        add_user(&mut users, Some(b""));
        let mut source = b"new-user".to_vec();
        add_user(&mut users, Some(&source));
        source[0] = b'X';

        assert_eq!(users, vec![b"existing".to_vec(), b"new-user".to_vec()]);
    }

    #[test]
    fn os_get_username_returns_something_nonempty() {
        // Whichever branch fires (a real name or the numeric
        // fallback), the result must never be empty.
        let result = os_get_username();
        let bytes: &[u8] = match &result {
            Ok(b) | Err(b) => b,
        };
        assert!(!bytes.is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn os_get_username_is_the_numeric_fallback_on_windows() {
        // See os_get_username's own doc comment: Windows hard-codes
        // uid 0 (matching a known upstream limitation), so this
        // always reports Err("0") on this platform.
        assert_eq!(os_get_username(), Err(b"0".to_vec()));
    }

    #[cfg(windows)]
    #[test]
    fn os_get_uname_falls_back_to_numeric_on_windows() {
        assert_eq!(os_get_uname(42), Err(b"42".to_vec()));
    }

    #[test]
    fn os_get_userdir_of_an_empty_name_is_none() {
        assert_eq!(os_get_userdir(b""), None);
    }

    #[test]
    fn os_get_userdir_of_an_implausible_name_is_none() {
        // No such user can exist, on any platform.
        assert_eq!(
            os_get_userdir(b"nero_test_no_such_user_9c1f4a2b"),
            None
        );
    }

    #[test]
    fn os_get_userdir_rejects_an_interior_nul() {
        // An interior NUL would silently truncate the C-string lookup
        // and resolve a DIFFERENT user than asked for, so it is
        // refused outright.
        assert_eq!(os_get_userdir(b"root\0evil"), None);
    }

    #[cfg(windows)]
    #[test]
    fn os_get_userdir_is_always_none_on_windows() {
        // The original's lookup is HAVE_PWD_FUNCS-gated (Unix-only),
        // so it is compiled out entirely here and always returns NULL.
        assert_eq!(os_get_userdir(b"Administrator"), None);
    }

    #[cfg(unix)]
    #[test]
    fn os_get_userdir_of_the_current_user_matches_its_passwd_entry() {
        // Look the CURRENT user up by name and confirm a non-empty,
        // absolute home directory comes back - exercising the real
        // getpwnam path rather than only the failure branches.
        let Ok(name) = os_get_username() else {
            // No real passwd entry for this uid (possible in some
            // minimal containers); nothing to assert against.
            return;
        };
        let Some(dir) = os_get_userdir(&name) else {
            // A user can legitimately have no passwd entry reachable
            // by name even when reachable by uid.
            return;
        };
        assert!(!dir.is_empty());
        assert_eq!(dir[0], b'/');
    }

    #[test]
    fn match_user_distinguishes_none_partial_and_full_matches() {
        let users = vec![b"alice".to_vec(), b"bob".to_vec()];
        assert_eq!(match_user_in(&users, b"carol"), 0);
        assert_eq!(match_user_in(&users, b"ali"), 1);
        assert_eq!(match_user_in(&users, b"alice"), 2);
        assert_eq!(match_user_in(&users, b""), 1);
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call passwd/NetUserEnum FFI")]
    fn username_cache_and_index_accessor_agree() {
        for (index, expected) in USERS.iter().enumerate() {
            assert_eq!(get_users(index as i32), Some(expected.as_slice()));
        }
        assert_eq!(get_users(-1), None);
        assert_eq!(get_users(i32::MAX), None);
    }
}
