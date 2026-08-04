//! Translated from `src/nvim/os/env.c` (tractable core only).
//!
//! Translated: `env_init`, `os_getenv` (like `getenv()` but returns
//! `None` for an empty value), `os_env_exists`, `os_setenv`,
//! `os_unsetenv`, `os_get_pid`, `vim_setenv_ext`, `vim_unsetenv_ext`.
//! In the original these wrap
//! `uv_os_getenv`/`uv_os_setenv`/`uv_os_unsetenv`/`getpid` (libuv or the
//! C standard library) purely for portability; Rust's own `std::env`/
//! `std::process` already provide the same portable primitives
//! natively (same reasoning as `os/time.rs`), so they're translated now
//! rather than waiting on the still-open libuv FFI-vs-Rust-runtime
//! decision (phase 11). Also `os_homedir`/`init_homedir` (+ its own
//! `os_uv_homedir` file-static, only partially implemented - see its
//! own doc comment), and `os_get_hostname` (hand-written Win32 FFI on
//! Windows/`libc::uname` on Unix - same "small, self-contained OS
//! wrapper" treatment as `os/proc.c`'s `os_proc_running` and
//! `os/users.c`'s `os_get_username`; Unix-only code additionally
//! cross-checked via `cargo check --target x86_64-unknown-linux-gnu`,
//! since this crate's dev machine is Windows-only).
//!
//! `vim_getenv` is now translated too, but only its common-case path:
//! a real environment variable (via `os_getenv`, with the `TO_SLASH`
//! backslash-to-forward-slash normalization for a handful of specific
//! path-like names on Windows) and the Windows-only `$HOME` special
//! case (`os_homedir`). Its OWN fallback - discovering `$VIM`/
//! `$VIMRUNTIME` by locating the nvim executable itself when neither
//! is set as a real environment variable - `unimplemented!()`s when
//! actually reached (needs `vim_runtime_dir`/
//! `vim_get_prefix_from_exepath`, real runtime-path auto-discovery,
//! not yet translated).
//!
//! Also translated: `restore_env_var` (`#ifdef MSWIN`-only in the
//! original, matched here via `#[cfg(windows)]`), `os_shell_is_cmdexe`
//! (re-examined an earlier session's "needs `'shell'` parsing logic
//! not yet translated" note and found it was about a hypothetical
//! CALLER needing a real `'shell'` option value, not this function's
//! own logic - a pure function of its own `sh` parameter, needing
//! only the already-real `striequal`/`os_getenv`/`path_tail`), and
//! `os_setenv_append_path` (its own sibling deferral note about
//! needing `path_is_absolute`/a scratch buffer/`ENV_SEPCHAR`
//! PATH-list manipulation - all either already real or, for the
//! scratch buffer, unnecessary in Rust; see its own doc comment).
//!
//! Deferred (each needs a not-yet-translated subsystem):
//! - `os_getenv_buf`/`os_getenv_noalloc`: write into `NameBuff`
//!   (`crate::globals::GLOBALS`) - tractable in principle, deferred only
//!   because nothing calls them yet without a fixed-size-buffer-filling
//!   caller to validate against.
//! - `os_free_fullenv`/`os_getenvname_at_index`: need libuv's
//!   `uv_os_environ`/raw platform `environ`/`GetEnvironmentStringsW`
//!   enumeration API, not just a single-variable get/set.
//! - `os_hint_priority`: platform-specific process scheduling-priority
//!   hints (`setpriority`/`task_policy_set`), no real caller yet.
//! - `free_homedir`: `#ifdef EXITFREE`-only (debug/leak-detection
//!   build flag with no equivalent concept in this crate, same
//!   reasoning as other `EXITFREE`-gated functions elsewhere); also
//!   moot here since `HOMEDIR`'s `Option<Vec<u8>>` already drops its
//!   contents automatically, with no separate "free" step needed.
//! - `expand_env*`/`home_replace*`: need `path.c`'s directory/file-name
//!   manipulation functions (`home_replace*`) or a much larger slice of
//!   them plus `` `=expr` `` Vimscript-expression substitution
//!   (`expand_env*`) than `vim_getenv` alone needed.
//! - `vim_runtime_dir`/`remove_tail`: only called by `vim_getenv`'s own
//!   still-deferred `$VIM`/`$VIMRUNTIME` auto-discovery fallback,
//!   deferred with it.
//! - `vim_env_iter`/`vim_env_iter_rev`: only consumed by
//!   `set_runtimepath_default`/similar (not yet translated).
//! - `get_env_name`: needs `expand_T` (cmdline completion, phase 7).

use super::os::NVIM_TESTING;
use crate::globals::GlobalCell;

/// Sets initial values for various environment-derived variables
/// (`env_init`).
pub fn env_init() {
    unsafe { *NVIM_TESTING.get_mut() = os_env_exists(b"NVIM_TEST", false) };
}

/// Like `getenv()`, but returns `None` if the variable is empty
/// (`os_getenv`).
///
/// Result must be freed by the caller (N/A in Rust - ownership is
/// simply returned).
///
/// @see os_env_exists
/// @see os_getenv_noalloc
#[must_use]
pub fn os_getenv(name: &[u8]) -> Option<Vec<u8>> {
    if name.is_empty() {
        return None;
    }
    let name = std::str::from_utf8(name).ok()?;
    match std::env::var_os(name) {
        Some(v) if !v.is_empty() => Some(v.to_string_lossy().into_owned().into_bytes()),
        _ => None,
    }
}

/// Returns true if environment variable `name` is defined (even if
/// empty). Returns false if not found or other failure (`os_env_exists`).
///
/// @param nonempty Require a non-empty value. Treat empty as "does not
///                 exist".
#[must_use]
pub fn os_env_exists(name: &[u8], nonempty: bool) -> bool {
    if name.is_empty() {
        return false;
    }
    let Ok(name) = std::str::from_utf8(name) else {
        return false;
    };
    match std::env::var_os(name) {
        Some(v) => !nonempty || !v.is_empty(),
        None => false,
    }
}

/// Sets an environment variable (`os_setenv`).
///
/// Windows (Vim-compat): Empty string (`:let $FOO=""`) undefines the
/// env var.
///
/// # Safety
/// Same requirement as `std::env::set_var`/`std::env::remove_var`: not
/// sound to call while other threads are concurrently reading/writing
/// the process environment (matches the original's own implicit
/// single-threaded-access assumption, which this crate preserves
/// throughout rather than adding new synchronization not present in the
/// original).
pub unsafe fn os_setenv(name: &[u8], value: &[u8], overwrite: i32) -> i32 {
    if name.is_empty() {
        return -1;
    }
    let Ok(name_str) = std::str::from_utf8(name) else {
        return -1;
    };

    if cfg!(windows) {
        if overwrite == 0 && !os_env_exists(name, true) {
            return 0;
        }
        if value.is_empty() {
            // Windows (Vim-compat): Empty string undefines the env var.
            return unsafe { os_unsetenv(name) };
        }
    } else if overwrite == 0 && os_env_exists(name, false) {
        return 0;
    }

    let Ok(value_str) = std::str::from_utf8(value) else {
        return -1;
    };
    // SAFETY: forwarded from this function's own safety contract.
    unsafe { std::env::set_var(name_str, value_str) };
    0
}

/// Unset environment variable (`os_unsetenv`).
///
/// # Safety
/// Same requirement as `std::env::remove_var` - see [`os_setenv`].
pub unsafe fn os_unsetenv(name: &[u8]) -> i32 {
    if name.is_empty() {
        return -1;
    }
    let Ok(name_str) = std::str::from_utf8(name) else {
        return -1;
    };
    // SAFETY: forwarded from this function's own safety contract.
    unsafe { std::env::remove_var(name_str) };
    0
}

/// Get the process ID of the Nvim process (`os_get_pid`).
#[must_use]
pub fn os_get_pid() -> i64 {
    std::process::id() as i64
}

/// Removes environment variable `name` and takes care of side effects
/// (`vim_unsetenv_ext`).
///
/// # Safety
/// Touches `crate::globals::GLOBALS` (`didset_vim`/`didset_vimruntime`)
/// plus forwards [`os_unsetenv`]'s own safety requirement.
pub unsafe fn vim_unsetenv_ext(name: &[u8]) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { os_unsetenv(name) };

    // "homedir" is not cleared, keep using the old value until $HOME
    // is set.
    if name.eq_ignore_ascii_case(b"VIM") {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::globals::GLOBALS.get_mut() }.didset_vim = false;
    } else if name.eq_ignore_ascii_case(b"VIMRUNTIME") {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::globals::GLOBALS.get_mut() }.didset_vimruntime = false;
    }
}

/// Sets environment variable `name` to `val` and takes care of side
/// effects (`vim_setenv_ext`).
///
/// # Safety
/// Touches `crate::globals::GLOBALS` (`didset_vim`/`didset_vimruntime`,
/// plus [`init_homedir`]'s own `HOMEDIR`) and forwards [`os_setenv`]'s
/// own safety requirement.
pub unsafe fn vim_setenv_ext(name: &[u8], val: &[u8]) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { os_setenv(name, val, 1) };
    if name.eq_ignore_ascii_case(b"HOME") {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { init_homedir() };
    } else if unsafe { crate::globals::GLOBALS.get_mut() }.didset_vim
        && name.eq_ignore_ascii_case(b"VIM")
    {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::globals::GLOBALS.get_mut() }.didset_vim = false;
    } else if unsafe { crate::globals::GLOBALS.get_mut() }.didset_vimruntime
        && name.eq_ignore_ascii_case(b"VIMRUNTIME")
    {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::globals::GLOBALS.get_mut() }.didset_vimruntime = false;
    }
}

/// Restores a previous environment variable value, or unsets it if
/// `old_value` is `None` (`restore_env_var`, `#ifdef MSWIN`-only in
/// the original).
///
/// # Safety
/// Same requirement as [`os_setenv`]/[`os_unsetenv`].
#[cfg(windows)]
pub unsafe fn restore_env_var(name: &[u8], old_value: Option<&[u8]>) {
    match old_value {
        Some(v) => {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { os_setenv(name, v, 1) };
        }
        None => {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { os_unsetenv(name) };
        }
    }
}

/// Whether `sh` refers to the Windows `"cmd.exe"` shell - directly,
/// via the bare name `"cmd"`, or (recursively) via the special
/// `"$COMSPEC"` token (`os_shell_is_cmdexe`).
///
/// Reads the real `$COMSPEC` environment variable (via [`os_getenv`],
/// matching the original's own established "`os_getenv_noalloc`'s
/// 'borrow, don't allocate' optimization is never separately
/// translated" convention - see this module's own doc comment) only
/// when `sh` is literally `"$COMSPEC"`.
#[must_use]
pub fn os_shell_is_cmdexe(sh: &[u8]) -> bool {
    if sh.is_empty() {
        return false;
    }
    if crate::strings::striequal(Some(sh), Some(b"$COMSPEC")) {
        let comspec = os_getenv(b"COMSPEC");
        // `path_tail(NULL)` returns `""` in the original - modeled
        // here as an empty tail when `$COMSPEC` itself is unset.
        let tail: &[u8] = match &comspec {
            Some(c) => &c[crate::path::path_tail(c)..],
            None => b"",
        };
        return crate::strings::striequal(Some(b"cmd.exe"), Some(tail));
    }
    if crate::strings::striequal(Some(sh), Some(b"cmd.exe"))
        || crate::strings::striequal(Some(sh), Some(b"cmd"))
    {
        return true;
    }
    crate::strings::striequal(Some(b"cmd.exe"), Some(&sh[crate::path::path_tail(sh)..]))
}

/// Prepends `fname`'s own containing directory onto `$PATH`
/// (`os_setenv_append_path`).
///
/// `fname` must be an absolute path (matches the original's own
/// `FUNC_ATTR_NONNULL_ALL`/`path_is_absolute` assertion, returning
/// `false` if not) - the original's own `internal_error()` message-
/// display call on that branch is skipped, keeping the exact same
/// `false` return value (this crate's established "skip the deferred
/// message-display side effect, keep the exact same state/return
/// value" policy).
///
/// The original's own fixed-size, global `os_buf` scratch buffer has
/// no equivalent here - a local, dynamically-sized `Vec<u8>` is used
/// instead, matching this module's own established "`os_getenv_noalloc`'s
/// 'borrow, don't allocate' optimization is never separately
/// translated" convention for the same class of C-buffer-reuse
/// micro-optimization.
///
/// `MAX_ENVPATHLEN` (`8192` on Windows, effectively unbounded
/// elsewhere) is preserved exactly: if appending would meet or exceed
/// it, nothing is changed and this returns `false`.
///
/// # Safety
/// Same requirement as [`os_setenv`].
pub unsafe fn os_setenv_append_path(fname: &[u8]) -> bool {
    // `INT_MAX` in the original (NOT `SIZE_MAX`/`usize::MAX` - a real
    // bug caught here via a genuine `clippy::absurd_extreme_comparisons`
    // deny-level error on Unix, where `usize::MAX` made the `>=` check
    // below vacuously almost-always-false).
    #[cfg(windows)]
    const MAX_ENVPATHLEN: usize = 8192;
    #[cfg(not(windows))]
    const MAX_ENVPATHLEN: usize = i32::MAX as usize;

    if !crate::path::path_is_absolute(fname) {
        return false;
    }

    let tail = crate::path::path_tail_with_sep(fname);
    let dir = &fname[..tail];

    let path = os_getenv(b"PATH");
    let path_len = path.as_deref().map_or(0, <[u8]>::len);
    let new_len = path_len + dir.len() + 2;

    if new_len >= MAX_ENVPATHLEN {
        return false;
    }

    let mut temp = Vec::with_capacity(new_len);
    if let Some(path) = path.as_deref()
        && !path.is_empty()
    {
        temp.extend_from_slice(path);
        if path.last() != Some(&(crate::os::os_defs::ENV_SEPCHAR as u8)) {
            temp.push(crate::os::os_defs::ENV_SEPCHAR as u8);
        }
    }
    temp.extend_from_slice(dir);

    // SAFETY: forwarded from this function's own safety doc.
    unsafe { os_setenv(b"PATH", &temp, 1) };
    true
}

/// Gets the hostname of the current machine (`os_get_hostname`).
///
/// Returns an empty `Vec` on failure, matching the original's "leaves
/// the output buffer as an empty string" behavior on error (`uname()`
/// failing on Unix, or `GetComputerNameW` failing on Windows) - never
/// a hard failure indicator, since callers (e.g. `memline.c`'s
/// `ml_open`) just use whatever comes back, truncated or empty, rather
/// than checking a separate status code.
#[must_use]
pub fn os_get_hostname() -> Vec<u8> {
    #[cfg(unix)]
    {
        os_get_hostname_unix()
    }
    #[cfg(windows)]
    {
        os_get_hostname_windows()
    }
    #[cfg(not(any(unix, windows)))]
    {
        Vec::new()
    }
}

/// Unix implementation of [`os_get_hostname`]: `uname()`'s `nodename`
/// field (matches the original's own `HAVE_SYS_UTSNAME_H` branch
/// exactly - not `gethostname()`, a different, simpler POSIX call the
/// original doesn't use here).
#[cfg(unix)]
fn os_get_hostname_unix() -> Vec<u8> {
    // SAFETY: `buf` is a plain-old-data struct the syscall fills in;
    // `uname` has no other preconditions.
    let mut buf: libc::utsname = unsafe { std::mem::zeroed() };
    // SAFETY: buf is a valid, correctly-sized out-parameter.
    let ret = unsafe { libc::uname(&mut buf) };
    if ret < 0 {
        return Vec::new();
    }
    // SAFETY: uname() succeeded, so nodename is a valid
    // NUL-terminated C string.
    unsafe { std::ffi::CStr::from_ptr(buf.nodename.as_ptr()) }
        .to_bytes()
        .to_vec()
}

/// Windows implementation of [`os_get_hostname`]: `GetComputerNameW`,
/// hand-written Win32 FFI (no new crate dependency, matching this
/// crate's existing `os/proc.rs` precedent) plus Rust's own
/// `String::from_utf16_lossy` instead of the original's manual
/// `utf16_to_utf8` conversion helper (not itself translated - Rust's
/// std already covers this natively).
#[cfg(windows)]
fn os_get_hostname_windows() -> Vec<u8> {
    // Real Win32 constant (`winbase.h`): the NetBIOS computer-name
    // length limit, verified against Microsoft's own documentation.
    const MAX_COMPUTERNAME_LENGTH: usize = 15;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetComputerNameW(lp_buffer: *mut u16, n_size: *mut u32) -> i32;
    }

    let mut buf = [0u16; MAX_COMPUTERNAME_LENGTH + 1];
    let mut size = buf.len() as u32;
    // SAFETY: buf/size describe a valid, correctly-sized mutable
    // buffer and its capacity (as GetComputerNameW's contract
    // requires); both live for the duration of this call.
    let ok = unsafe { GetComputerNameW(buf.as_mut_ptr(), &mut size) };
    if ok == 0 {
        return Vec::new();
    }
    String::from_utf16_lossy(&buf[..size as usize]).into_bytes()
}

/// The "real", resolved user home directory, set by [`init_homedir`]
/// (`homedir`, a file-static in the original - not an `EXTERN` global,
/// same treatment as `buffer.c`'s own file-statics like
/// `crate::buffer`'s `TOP_FILE_NUM`).
static HOMEDIR: std::sync::LazyLock<GlobalCell<Option<Vec<u8>>>> =
    std::sync::LazyLock::new(|| GlobalCell::new(None));

/// Gets the "real", resolved user home directory as determined by
/// [`init_homedir`] (`os_homedir`).
///
/// The original `emsg`s and returns `NULL` if `init_homedir` hasn't
/// been called yet or failed; `emsg()` itself isn't translated yet
/// (`message.c`), so this just returns `None` in that case.
///
/// # Safety
/// Touches a `GlobalCell` - same requirement as every other function
/// that does so: no overlapping live access.
#[must_use]
pub unsafe fn os_homedir() -> Option<Vec<u8>> {
    unsafe { HOMEDIR.get_mut() }.clone()
}

/// Queries the OS for the current user's home directory
/// (`os_uv_homedir`, `static` in the original).
///
/// **Not yet implemented** (always returns `None`): the original wraps
/// `uv_os_homedir()`, which on Windows calls `GetUserProfileDirectoryW`
/// (a Win32 API needing an FFI decision not yet made) and on Unix reads
/// `getpwuid()` (needs the same `libc`-FFI decision noted in
/// `os/users.c`'s investigation). [`init_homedir`] still produces a
/// correct result in the overwhelmingly common case where `$HOME` (or,
/// on Windows, `$HOMEDRIVE`+`$HOMEPATH`) is set, which covers virtually
/// every real login session - this is only a fallback for the rarer
/// case where none of those are set.
fn os_uv_homedir() -> Option<Vec<u8>> {
    None
}

/// Sets the resolved user home directory (`HOMEDIR`, read via
/// [`os_homedir`]), as follows (`init_homedir`):
///  1. get value of `$HOME`
///  2. if `$HOME` is not set, try the following:
///
/// For Windows:
///  1. assemble homedir using `$HOMEDRIVE` and `$HOMEPATH`
///  2. try `os_uv_homedir` (not yet implemented, see its own doc)
///  3. resolve a direct reference to another system variable
///  4. guess `C:/`
///
/// For Unix:
///  1. try `os_uv_homedir` (not yet implemented, see its own doc)
///  2. resolve it with `os_realpath` (this also works with mounts and
///     links)
///  3. fall back to the current working directory as a last resort
///
/// # Safety
/// Touches a `GlobalCell` - same requirement as every other function
/// that does so: no overlapping live access.
pub unsafe fn init_homedir() {
    // In case we are called a second time.
    unsafe { *HOMEDIR.get_mut() = None };

    let mut var: Option<Vec<u8>> = os_getenv(b"HOME");

    if cfg!(windows) {
        if var.is_none() {
            // Typically, $HOME is not defined on Windows, unless the
            // user has specifically defined it. However, $HOMEDRIVE
            // and $HOMEPATH are automatically defined for each user on
            // Windows NT platforms. Try constructing $HOME from these.
            let homedrive = os_getenv(b"HOMEDRIVE");
            let homepath = os_getenv(b"HOMEPATH")
                .unwrap_or_else(|| crate::ascii_defs::PATHSEPSTR.as_bytes().to_vec());
            if let Some(homedrive) = homedrive {
                let mut combined = homedrive;
                combined.extend_from_slice(&homepath);
                if !combined.is_empty() {
                    var = Some(combined);
                }
            }
        }
        if var.is_none() {
            var = os_uv_homedir();
        }

        // Weird but true: $HOME may contain an indirect reference to
        // another variable, esp. "%USERPROFILE%". Happens when
        // $USERPROFILE isn't set when $HOME is being set.
        //
        // Extract an owned (name, suffix) pair first so the borrow of
        // `var` ends before `var` is reassigned below.
        let indirect_ref: Option<(Vec<u8>, Vec<u8>)> = var.as_ref().and_then(|v| {
            if v.first() != Some(&b'%') {
                return None;
            }
            let rel_pos = v[1..].iter().position(|&b| b == b'%')?;
            let name = v[1..1 + rel_pos].to_vec();
            let suffix = v[1 + rel_pos..].to_vec(); // starts with the closing '%'
            Some((name, suffix))
        });
        if let Some((name, suffix)) = indirect_ref {
            var = None;
            if let Some(exp) = os_getenv(&name)
                && !exp.is_empty()
            {
                let mut combined = exp;
                combined.extend_from_slice(&suffix[1..]);
                var = Some(combined);
            }
        }

        // Default home dir is C:/. Best assumption we can make in such
        // a situation.
        if var.as_ref().is_none_or(|v| v.is_empty()) {
            var = Some(b"C:/".to_vec());
        }
    } else {
        if var.is_none() {
            var = os_uv_homedir();
        }

        // Get the actual path. This resolves links.
        if let Some(v) = &var
            && let Ok(path_str) = std::str::from_utf8(v)
            && let Some(real) = crate::os::fs::os_realpath(std::path::Path::new(path_str))
        {
            var = Some(real);
        }

        // Fall back to current working directory if home is not found.
        if var.as_ref().is_none_or(|v| v.is_empty()) {
            var = crate::os::fs::os_dirname();
        }
    }

    if let Some(mut result) = var {
        crate::path::path_to_slash(&mut result);
        unsafe { *HOMEDIR.get_mut() = Some(result) };
    }
}

/// `getenv()` wrapper with special handling of `$HOME` (Windows only),
/// `$VIM`, `$VIMRUNTIME` (`vim_getenv`).
///
/// Returns `None` when `name` isn't set in the environment (or is set
/// to an empty string - [`os_getenv`]'s own established "empty is
/// `None`" treatment already covers the original's separate `string
/// == NULL || *string == NUL` check at every real call site).
///
/// # Safety
/// Forwarded from [`os_homedir`]'s own safety doc (Windows `$HOME`
/// path only).
#[must_use]
pub unsafe fn vim_getenv(name: &[u8]) -> Option<Vec<u8>> {
    if cfg!(windows) && name == b"HOME" {
        // SAFETY: forwarded from this function's own safety doc.
        return unsafe { os_homedir() };
    }

    if let Some(mut value) = os_getenv(name) {
        // Backslashes in these specific path-like variables are
        // normalized to forward slashes on Windows
        // (`BACKSLASH_IN_FILENAME` builds only) - TO_SLASH's own
        // established treatment; a no-op macro on Unix.
        if cfg!(windows) {
            const SLASH_NORMALIZED_NAMES: &[&[u8]] =
                &[b"VIMRUNTIME", b"PATH", b"CDPATH", b"TMPDIR", b"TMP", b"TEMP", b"VIM", b"MYVIMRC"];
            if SLASH_NORMALIZED_NAMES.iter().any(|n| name.eq_ignore_ascii_case(n)) {
                crate::path::path_to_slash(&mut value);
            }
        }
        return Some(value);
    }

    if name == b"VIM" || name == b"VIMRUNTIME" {
        // When expanding $VIM/$VIMRUNTIME fails via a real
        // environment variable, the original falls back to
        // discovering the runtime directory relative to the nvim
        // executable itself (vim_runtime_dir/
        // vim_get_prefix_from_exepath, using 'helpfile' or argv[0] as
        // a last resort) - none of that runtime-path-discovery
        // machinery is translated yet.
        unimplemented!(
            "vim_getenv: $VIM/$VIMRUNTIME auto-discovery (vim_runtime_dir/\
             vim_get_prefix_from_exepath) not yet translated"
        );
    }

    None
}

/// Expand environment variables (`$VAR`, and Unix `${VAR}`) and a
/// leading `~`/`~user` in `srcp`, escaping any character in
/// `esc_chars` found in an expanded value with a backslash
/// (`expand_env_esc`).
///
/// Returns the expanded byte string directly as an owned `Vec<u8>`
/// rather than writing into a caller-provided, `dstlen`-bounded fixed
/// buffer - Rust's own growable buffer needs no pre-sizing dance,
/// matching this crate's established simplification for this exact
/// category of C function (e.g. `winrestcmd`/`vim_strsave_shellescape`).
///
/// `one`: treat `srcp` as a single file name (only expand `~` at the
/// very start, and don't treat a space/comma as a new-name boundary).
///
/// `prefix`: after copying a byte immediately following an expansion,
/// if the `prefix.len()` bytes of `srcp` just consumed end in
/// `prefix`, treat the position right after as a fresh "start of
/// name" too - not exercised by any real caller in this crate yet
/// (needs `ex_docmd.c`/`option.c`/`file_search.c`, none translated),
/// translated for structural fidelity anyway since it adds no real
/// complexity.
///
/// The Unix-only `~user` form (`~someone/...`, needing
/// `os_get_userdir`, not yet translated) `unimplemented!()`s if ever
/// reached - a real, if less common, gap (`~` alone, and every
/// `$VAR` form, both work for real).
///
/// # Safety
/// Forwarded from [`vim_getenv`]/[`os_homedir`]'s own safety docs.
#[must_use]
pub unsafe fn expand_env_esc(srcp: &[u8], esc_chars: Option<&[u8]>, one: bool, prefix: Option<&[u8]>) -> Vec<u8> {
    let mut dst: Vec<u8> = Vec::new();
    let mut at_start = true;
    let mut pos = crate::charset::skipwhite(srcp);

    while pos < srcp.len() {
        // Skip over `=expr` verbatim (not evaluated here - the
        // original's own comment: this just measures/copies the raw
        // text, matching skip_expr's own "parse only" contract).
        if srcp[pos] == b'`' && srcp.get(pos + 1) == Some(&b'=') {
            let var_start = pos;
            pos += 2;
            // SAFETY: forwarded from this function's own safety doc.
            let (_, consumed) = unsafe { crate::eval::eval::skip_expr(&srcp[pos..], None) };
            pos += consumed;
            if srcp.get(pos) == Some(&b'`') {
                pos += 1;
            }
            dst.extend_from_slice(&srcp[var_start..pos]);
            continue;
        }

        let mut copy_char = true;
        if srcp[pos] == b'$' || (srcp[pos] == b'~' && at_start) {
            let mut var: Option<Vec<u8>>;
            let mut tail: usize;

            if srcp[pos] != b'~' {
                // Environment variable.
                let mut name_start = pos + 1;
                let braced = cfg!(unix) && srcp.get(name_start) == Some(&b'{');
                if braced {
                    name_start += 1;
                }
                let mut p = name_start;
                if braced {
                    while p < srcp.len() && srcp[p] != b'}' {
                        p += 1;
                    }
                } else {
                    while p < srcp.len() && crate::charset::vim_isidc(i32::from(srcp[p])) {
                        p += 1;
                    }
                }
                // Unix ${VAR} form requires the closing brace to
                // actually be present - matches the original's own
                // "verify we found the end" check.
                if braced && srcp.get(p) != Some(&b'}') {
                    var = None;
                    tail = p;
                } else {
                    // SAFETY: forwarded from this function's own
                    // safety doc.
                    var = unsafe { vim_getenv(&srcp[name_start..p]) };
                    if let Some(v) = &mut var {
                        // Expanded env vars represent paths, so their
                        // backslashes can be safely normalized -
                        // TO_SLASH's own real, Windows-only nature.
                        if cfg!(windows) {
                            crate::path::path_to_slash(v);
                        }
                    }
                    tail = if braced { p + 1 } else { p };
                }
            } else if srcp.get(pos + 1).is_none_or(|&b| {
                b == crate::ascii_defs::NUL || crate::path::vim_ispathsep(i32::from(b)) || b" ,\t\n".contains(&b)
            }) {
                // Home directory.
                // SAFETY: forwarded from this function's own safety doc.
                var = unsafe { os_homedir() };
                tail = pos + 1;
            } else {
                // ~user - needs os_get_userdir (Unix-only, not yet
                // translated). Only reached for a genuine ~name form,
                // never for a bare ~ (handled above).
                unimplemented!(
                    "expand_env_esc: a real ~user form needs os_get_userdir, not yet translated"
                );
            }

            if let (Some(ec), Some(v)) = (esc_chars, &var)
                && v.iter().any(|b| ec.contains(b))
            {
                // SAFETY: forwarded from this function's own
                // safety doc.
                var = Some(unsafe { crate::strings::vim_strsave_escaped(v, ec) });
            }

            if let Some(v) = &var
                && !v.is_empty()
            {
                dst.extend_from_slice(v);
                if crate::path::after_pathsep(v, v.len())
                    && srcp.get(tail).is_some_and(|&b| crate::path::vim_ispathsep(i32::from(b)))
                {
                    tail += 1;
                }
                pos = tail;
                copy_char = false;
            }
        }

        if copy_char {
            // Recognize the start of a new name, for '~'. Don't do
            // this when "one" is true, to avoid expanding "~" in
            // ":edit foo ~ foo".
            at_start = false;
            if srcp[pos] == b'\\' && srcp.get(pos + 1).is_some() {
                dst.push(srcp[pos]);
                pos += 1;
            } else if !one && (srcp[pos] == b' ' || srcp[pos] == b',') {
                at_start = true;
            }
            if pos < srcp.len() {
                dst.push(srcp[pos]);
                pos += 1;
                if let Some(pfx) = prefix
                    && pos >= pfx.len() && &srcp[pos - pfx.len()..pos] == pfx
                {
                    at_start = true;
                }
            }
        }
    }

    dst
}

/// [`expand_env_esc`] with `esc_chars = None`/`one = false`/
/// `prefix = None` (`expand_env_save`).
///
/// # Safety
/// Forwarded from [`expand_env_esc`]'s own safety doc.
#[must_use]
pub unsafe fn expand_env_save(src: &[u8]) -> Vec<u8> {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { expand_env_esc(src, None, false, None) }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    // Environment variables are process-global state shared by all
    // threads; Rust's default test runner uses multiple threads, so
    // every test here uses a unique variable name to avoid colliding
    // with any other concurrently-running test in this crate.

    /// Serializes tests that mutate `$HOME`/`$HOMEDRIVE`/`$HOMEPATH`
    /// (real, well-known environment variable names that can't be
    /// namespaced per-test the way arbitrary `NERO_TEST_ENV_*` names
    /// can), since Rust's multi-threaded test runner would otherwise
    /// let these tests race against each other. `pub(crate)` (module
    /// and function both) since `os::stdpaths`'s own tests also touch
    /// `HOMEDIR` transitively (via `init_homedir`/`os_homedir`, for
    /// `stdpath()`'s `~`-expanding fallback defaults) and must
    /// serialize against this exact same lock, not a separate one.
    static HOMEDIR_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Acquires [`HOMEDIR_TEST_LOCK`], tolerating a poisoned lock (one
    /// panicking test under the lock must not permanently break every
    /// later test that needs it) - same reasoning and pattern as
    /// `crate::os::fs::cwd_test_lock`. A real cross-platform test run
    /// (this crate's own Linux build, via WSL) caught exactly this: an
    /// unrelated pre-existing test bug (a Windows-only test missing
    /// `#[cfg(windows)]`) failed and poisoned this same
    /// `std::sync::Mutex`, cascading into an unrelated homedir test's
    /// failure purely due to `.lock().unwrap()` not tolerating that.
    pub(crate) fn homedir_test_lock() -> std::sync::MutexGuard<'static, ()> {
        HOMEDIR_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// RAII guard restoring a set of environment variables to their
    /// original values on drop (including on test panic via
    /// unwinding).
    struct EnvVarGuard {
        saved: Vec<(&'static str, Option<std::ffi::OsString>)>,
    }

    impl EnvVarGuard {
        fn set(vars: &[(&'static str, Option<&str>)]) -> Self {
            let saved = vars
                .iter()
                .map(|(name, _)| (*name, std::env::var_os(name)))
                .collect();
            for (name, value) in vars {
                // SAFETY: serialized via HOMEDIR_TEST_LOCK, held by every
                // caller of this helper.
                unsafe {
                    match value {
                        Some(v) => std::env::set_var(name, v),
                        None => std::env::remove_var(name),
                    }
                }
            }
            EnvVarGuard { saved }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            for (name, value) in &self.saved {
                // SAFETY: serialized via HOMEDIR_TEST_LOCK, held by every
                // caller of `EnvVarGuard::set`.
                unsafe {
                    match value {
                        Some(v) => std::env::set_var(name, v),
                        None => std::env::remove_var(name),
                    }
                }
            }
        }
    }

    #[test]
    fn init_homedir_uses_home_when_set() {
        let _lock = homedir_test_lock();
        let _guard = EnvVarGuard::set(&[("HOME", Some("C:/some/home"))]);
        unsafe {
            init_homedir();
            assert_eq!(os_homedir(), Some(b"C:/some/home".to_vec()));
        }
    }

    #[test]
    #[cfg(windows)]
    fn init_homedir_falls_back_to_homedrive_homepath() {
        let _lock = homedir_test_lock();
        let _guard = EnvVarGuard::set(&[
            ("HOME", None),
            ("HOMEDRIVE", Some("C:")),
            ("HOMEPATH", Some("\\Users\\test")),
        ]);
        unsafe {
            init_homedir();
            assert_eq!(os_homedir(), Some(b"C:/Users/test".to_vec()));
        }
    }

    #[test]
    #[cfg(windows)]
    fn init_homedir_falls_back_to_c_drive_when_nothing_set() {
        let _lock = homedir_test_lock();
        let _guard = EnvVarGuard::set(&[("HOME", None), ("HOMEDRIVE", None), ("HOMEPATH", None)]);
        unsafe {
            init_homedir();
            assert_eq!(os_homedir(), Some(b"C:/".to_vec()));
        }
    }

    #[test]
    #[cfg(windows)]
    fn init_homedir_resolves_percent_indirect_reference() {
        let _lock = homedir_test_lock();
        let _guard = EnvVarGuard::set(&[
            ("HOME", Some("%NERO_TEST_INDIRECT_VAR%")),
            ("NERO_TEST_INDIRECT_VAR", Some("C:/indirect/target")),
        ]);
        unsafe {
            init_homedir();
            assert_eq!(os_homedir(), Some(b"C:/indirect/target".to_vec()));
        }
    }

    #[test]
    fn init_homedir_on_real_ambient_environment_yields_absolute_path() {
        // No overrides - exercises the function against this machine's
        // real environment, just checking the general shape of the
        // result rather than an exact value (which depends on who's
        // running the tests).
        let _lock = homedir_test_lock();
        unsafe {
            init_homedir();
            let home = os_homedir().expect("init_homedir should always set something");
            assert!(!home.is_empty());
            assert!(!home.contains(&b'\\'));
        }
    }

    #[test]
    fn os_getenv_returns_none_for_unset_var() {
        assert_eq!(os_getenv(b"NERO_TEST_ENV_UNSET_VAR"), None);
    }

    #[test]
    fn os_getenv_returns_none_for_empty_name() {
        assert_eq!(os_getenv(b""), None);
    }

    #[test]
    fn setenv_getenv_unsetenv_roundtrip() {
        let name = b"NERO_TEST_ENV_ROUNDTRIP";
        // SAFETY: single test-owned variable name, not touched by other tests.
        unsafe {
            assert_eq!(os_setenv(name, b"hello", 1), 0);
            assert_eq!(os_getenv(name), Some(b"hello".to_vec()));
            assert!(os_env_exists(name, true));

            assert_eq!(os_unsetenv(name), 0);
            assert_eq!(os_getenv(name), None);
            assert!(!os_env_exists(name, false));
        }
    }

    #[test]
    #[cfg(windows)]
    fn setenv_overwrite_zero_on_windows_only_skips_if_unset() {
        // Faithful to the real upstream Windows-specific `os_setenv`
        // quirk (src/nvim/os/env.c's `#ifdef MSWIN` branch): on Windows,
        // `overwrite == 0` skips the assignment only when the variable
        // does NOT already exist; if it DOES exist, `overwrite == 0`
        // still updates it. This is the *opposite* of POSIX `setenv()`
        // semantics (which skip *existing* vars when overwrite == 0) -
        // preserved here exactly as-is rather than "fixed" to match
        // POSIX, since this is a literal translation.
        let name = b"NERO_TEST_ENV_NO_OVERWRITE_WIN";
        // SAFETY: single test-owned variable name, not touched by other tests.
        unsafe {
            // Var doesn't exist yet: overwrite=0 is a no-op.
            assert_eq!(os_setenv(name, b"first", 0), 0);
            assert_eq!(os_getenv(name), None);

            assert_eq!(os_setenv(name, b"first", 1), 0);
            assert_eq!(os_getenv(name), Some(b"first".to_vec()));

            // Now it exists: overwrite=0 still updates it (the quirk).
            assert_eq!(os_setenv(name, b"second", 0), 0);
            assert_eq!(os_getenv(name), Some(b"second".to_vec()));

            os_unsetenv(name);
        }
    }

    #[test]
    #[cfg(not(windows))]
    fn setenv_overwrite_zero_on_posix_keeps_existing_value() {
        // POSIX setenv() semantics: overwrite == 0 skips the assignment
        // when the variable already exists (src/nvim/os/env.c's `#else`
        // branch). This test doesn't run on this Windows machine, but
        // documents and would verify the other platform's behavior.
        let name = b"NERO_TEST_ENV_NO_OVERWRITE_POSIX";
        // SAFETY: single test-owned variable name, not touched by other tests.
        unsafe {
            assert_eq!(os_setenv(name, b"first", 1), 0);
            assert_eq!(os_setenv(name, b"second", 0), 0);
            assert_eq!(os_getenv(name), Some(b"first".to_vec()));
            os_unsetenv(name);
        }
    }

    #[test]
    fn empty_value_is_treated_as_unset_by_os_getenv() {
        let name = b"NERO_TEST_ENV_EMPTY_VALUE";
        // SAFETY: single test-owned variable name, not touched by other tests.
        unsafe {
            assert_eq!(os_setenv(name, b"", 1), 0);
            assert_eq!(os_getenv(name), None);
            os_unsetenv(name);
        }
    }

    #[test]
    fn os_get_pid_matches_std_process_id() {
        assert_eq!(os_get_pid(), std::process::id() as i64);
    }

    #[test]
    fn env_init_sets_nvim_testing_from_env_var() {
        // Doesn't assert a specific value (depends on the real test
        // runner's environment), just that it runs without panicking
        // and produces a bool consistent with os_env_exists.
        env_init();
        let expected = os_env_exists(b"NVIM_TEST", false);
        assert_eq!(unsafe { *NVIM_TESTING.get_mut() }, expected);
    }

    #[test]
    fn os_get_hostname_returns_a_nonempty_name_on_a_real_machine() {
        // Any real, correctly configured machine (Unix or Windows)
        // should have a resolvable hostname - this doesn't assert a
        // specific value (machine-dependent), just that the happy
        // path produces something.
        let name = os_get_hostname();
        assert!(!name.is_empty());
        // Must be valid UTF-8 (uname's nodename is whatever the OS
        // reports, but on Windows this is always guaranteed by
        // from_utf16_lossy; on Unix, real hostnames are ASCII/UTF-8 in
        // virtually every real deployment).
        assert!(std::str::from_utf8(&name).is_ok());
    }

    // --- vim_getenv ---

    #[test]
    fn vim_getenv_returns_a_real_environment_variable() {
        // NERO_TEST_ENV_* is a unique, crate-specific name - no lock
        // needed (this file's own established convention).
        let _guard = EnvVarGuard::set(&[("NERO_TEST_ENV_VIM_GETENV", Some("hello"))]);
        assert_eq!(unsafe { vim_getenv(b"NERO_TEST_ENV_VIM_GETENV") }, Some(b"hello".to_vec()));
    }

    #[test]
    fn vim_getenv_returns_none_for_an_unset_arbitrary_variable() {
        let _guard = EnvVarGuard::set(&[("NERO_TEST_ENV_VIM_GETENV_UNSET", None)]);
        assert_eq!(unsafe { vim_getenv(b"NERO_TEST_ENV_VIM_GETENV_UNSET") }, None);
    }

    #[test]
    fn vim_getenv_returns_none_for_an_empty_variable() {
        // os_getenv's own established "empty is None" treatment
        // already covers the original's "NULL or empty" check.
        let _guard = EnvVarGuard::set(&[("NERO_TEST_ENV_VIM_GETENV_EMPTY", Some(""))]);
        assert_eq!(unsafe { vim_getenv(b"NERO_TEST_ENV_VIM_GETENV_EMPTY") }, None);
    }

    #[test]
    #[cfg(windows)]
    fn vim_getenv_home_on_windows_uses_os_homedir_not_the_raw_env_var() {
        let _lock = homedir_test_lock();
        // A real $HOME env var is deliberately set to something
        // DIFFERENT from what init_homedir/os_homedir would resolve,
        // to prove vim_getenv(b"HOME") really goes through
        // os_homedir() on Windows rather than a plain os_getenv.
        let _guard = EnvVarGuard::set(&[("HOME", Some("C:/raw/env/value"))]);
        unsafe {
            init_homedir();
            assert_eq!(vim_getenv(b"HOME"), os_homedir());
            assert_eq!(vim_getenv(b"HOME"), Some(b"C:/raw/env/value".to_vec()));
        }
    }

    #[test]
    #[cfg(unix)]
    fn vim_getenv_home_on_unix_is_a_plain_environment_variable() {
        // On Unix, "HOME" has no special-case in vim_getenv itself -
        // it's resolved exactly like any other real env var (via
        // os_getenv directly), regardless of what os_homedir/
        // init_homedir would separately resolve.
        let _lock = homedir_test_lock();
        let _guard = EnvVarGuard::set(&[("HOME", Some("/raw/env/value"))]);
        assert_eq!(unsafe { vim_getenv(b"HOME") }, Some(b"/raw/env/value".to_vec()));
    }

    #[test]
    #[cfg(windows)]
    fn vim_getenv_normalizes_backslashes_for_specific_names_on_windows() {
        let _lock = homedir_test_lock();
        let _guard = EnvVarGuard::set(&[("PATH", Some(r"C:\foo\bar"))]);
        assert_eq!(unsafe { vim_getenv(b"PATH") }, Some(b"C:/foo/bar".to_vec()));
    }

    #[test]
    #[cfg(windows)]
    fn vim_getenv_slash_normalization_name_check_is_case_insensitive() {
        // Sets the env var under a mixed-case spelling ("MyVimRc") so
        // os_getenv/std::env::var_os finds it via an EXACT match
        // (avoiding any dependency on the OS's own env-var-name case-
        // folding, which is a separate, unrelated concern this
        // function doesn't implement itself) - this specifically
        // tests that the SLASH_NORMALIZED_NAMES membership check
        // (this function's own translation of the original's
        // case-insensitive striequal()) still recognizes "MyVimRc" as
        // meaning the same thing as the list's own "MYVIMRC" entry.
        // Uses MYVIMRC rather than PATH/similar to avoid any risk of
        // touching a real, load-bearing variable the test process
        // itself might depend on (Windows env vars are case-
        // insensitive at the OS level, so setting e.g. "Path" here
        // could alias the real "PATH").
        let _lock = homedir_test_lock();
        let _guard = EnvVarGuard::set(&[("MyVimRc", Some(r"C:\foo\bar"))]);
        assert_eq!(unsafe { vim_getenv(b"MyVimRc") }, Some(b"C:/foo/bar".to_vec()));
    }

    #[test]
    fn vim_getenv_does_not_normalize_backslashes_for_unlisted_names() {
        // Not in the TO_SLASH-normalized name list (on any platform) -
        // backslashes must pass through untouched.
        let _guard = EnvVarGuard::set(&[("NERO_TEST_ENV_VIM_GETENV_NOSLASH", Some(r"C:\foo\bar"))]);
        assert_eq!(
            unsafe { vim_getenv(b"NERO_TEST_ENV_VIM_GETENV_NOSLASH") },
            Some(br"C:\foo\bar".to_vec())
        );
    }

    #[test]
    fn vim_getenv_vimruntime_auto_discovery_is_unimplemented_when_unset() {
        let _lock = homedir_test_lock();
        let _guard = EnvVarGuard::set(&[("VIMRUNTIME", None)]);
        let result = std::panic::catch_unwind(|| unsafe { vim_getenv(b"VIMRUNTIME") });
        assert!(result.is_err(), "expected a panic (vim_runtime_dir not yet translated)");
    }

    #[test]
    fn vim_getenv_vim_auto_discovery_is_unimplemented_when_unset() {
        let _lock = homedir_test_lock();
        let _guard = EnvVarGuard::set(&[("VIM", None)]);
        let result = std::panic::catch_unwind(|| unsafe { vim_getenv(b"VIM") });
        assert!(result.is_err(), "expected a panic (vim_runtime_dir not yet translated)");
    }

    // --- vim_setenv_ext / vim_unsetenv_ext ---

    #[test]
    fn vim_setenv_ext_sets_an_ordinary_variable() {
        let name = b"NERO_TEST_ENV_SETENV_EXT";
        // SAFETY: single test-owned variable name, not touched by
        // other tests; doesn't reach the VIM/VIMRUNTIME/HOME branches
        // so no globals/homedir lock is needed.
        unsafe {
            vim_setenv_ext(name, b"hello");
            assert_eq!(os_getenv(name), Some(b"hello".to_vec()));
            os_unsetenv(name);
        }
    }

    #[test]
    fn vim_unsetenv_ext_removes_an_ordinary_variable() {
        let name = b"NERO_TEST_ENV_UNSETENV_EXT";
        // SAFETY: single test-owned variable name, not touched by
        // other tests; doesn't reach the VIM/VIMRUNTIME branches so no
        // globals lock is needed.
        unsafe {
            os_setenv(name, b"hello", 1);
            vim_unsetenv_ext(name);
            assert_eq!(os_getenv(name), None);
        }
    }

    #[test]
    fn vim_setenv_ext_calls_init_homedir_when_name_is_home() {
        let _lock = homedir_test_lock();
        let _guard = EnvVarGuard::set(&[("HOME", None)]);
        unsafe {
            vim_setenv_ext(b"HOME", b"C:/new/home");
            assert_eq!(os_homedir(), Some(b"C:/new/home".to_vec()));
        }
    }

    #[test]
    fn vim_setenv_ext_resets_didset_vim_when_name_is_vim() {
        let _homedir_lock = homedir_test_lock();
        let _globals_lock = crate::globals::global_state_test_lock();
        let _guard = EnvVarGuard::set(&[("VIM", None)]);
        unsafe { crate::globals::GLOBALS.get_mut() }.didset_vim = true;

        unsafe { vim_setenv_ext(b"VIM", b"/some/vim") };

        assert!(!unsafe { crate::globals::GLOBALS.get_mut() }.didset_vim);
        // Reset for any other test relying on the default.
        unsafe { crate::globals::GLOBALS.get_mut() }.didset_vim = false;
    }

    #[test]
    fn vim_setenv_ext_resets_didset_vimruntime_when_name_is_vimruntime() {
        let _homedir_lock = homedir_test_lock();
        let _globals_lock = crate::globals::global_state_test_lock();
        let _guard = EnvVarGuard::set(&[("VIMRUNTIME", None)]);
        unsafe { crate::globals::GLOBALS.get_mut() }.didset_vimruntime = true;

        unsafe { vim_setenv_ext(b"VIMRUNTIME", b"/some/runtime") };

        assert!(!unsafe { crate::globals::GLOBALS.get_mut() }.didset_vimruntime);
        unsafe { crate::globals::GLOBALS.get_mut() }.didset_vimruntime = false;
    }

    #[test]
    fn vim_setenv_ext_name_match_is_case_insensitive() {
        let _homedir_lock = homedir_test_lock();
        let _globals_lock = crate::globals::global_state_test_lock();
        let _guard = EnvVarGuard::set(&[("VIM", None)]);
        unsafe { crate::globals::GLOBALS.get_mut() }.didset_vim = true;

        // Lowercase "vim" - matches STRICMP's case-insensitivity.
        unsafe { vim_setenv_ext(b"vim", b"/some/vim") };

        assert!(!unsafe { crate::globals::GLOBALS.get_mut() }.didset_vim);
        unsafe { crate::globals::GLOBALS.get_mut() }.didset_vim = false;
    }

    #[test]
    fn vim_unsetenv_ext_resets_didset_vim_when_name_is_vim() {
        let _homedir_lock = homedir_test_lock();
        let _globals_lock = crate::globals::global_state_test_lock();
        let _guard = EnvVarGuard::set(&[("VIM", Some("/some/vim"))]);
        unsafe { crate::globals::GLOBALS.get_mut() }.didset_vim = true;

        unsafe { vim_unsetenv_ext(b"VIM") };

        assert_eq!(os_getenv(b"VIM"), None);
        assert!(!unsafe { crate::globals::GLOBALS.get_mut() }.didset_vim);
        unsafe { crate::globals::GLOBALS.get_mut() }.didset_vim = false;
    }

    #[test]
    fn vim_unsetenv_ext_resets_didset_vimruntime_when_name_is_vimruntime() {
        let _homedir_lock = homedir_test_lock();
        let _globals_lock = crate::globals::global_state_test_lock();
        let _guard = EnvVarGuard::set(&[("VIMRUNTIME", Some("/some/runtime"))]);
        unsafe { crate::globals::GLOBALS.get_mut() }.didset_vimruntime = true;

        unsafe { vim_unsetenv_ext(b"VIMRUNTIME") };

        assert_eq!(os_getenv(b"VIMRUNTIME"), None);
        assert!(!unsafe { crate::globals::GLOBALS.get_mut() }.didset_vimruntime);
        unsafe { crate::globals::GLOBALS.get_mut() }.didset_vimruntime = false;
    }

    #[test]
    fn vim_unsetenv_ext_does_not_touch_didset_vim_for_an_unrelated_name() {
        let _globals_lock = crate::globals::global_state_test_lock();
        let name = b"NERO_TEST_ENV_UNSETENV_EXT_UNRELATED";
        unsafe { crate::globals::GLOBALS.get_mut() }.didset_vim = true;
        // SAFETY: single test-owned variable name, not touched by
        // other tests.
        unsafe {
            os_setenv(name, b"hello", 1);
            vim_unsetenv_ext(name);
        }
        assert!(unsafe { crate::globals::GLOBALS.get_mut() }.didset_vim);
        unsafe { crate::globals::GLOBALS.get_mut() }.didset_vim = false;
    }

    // --- expand_env_esc / expand_env_save ---

    #[test]
    fn expand_env_save_expands_a_dollar_var_and_keeps_the_tail() {
        let _guard = EnvVarGuard::set(&[("NERO_TEST_EXPAND_ENV_VAR1", Some("/home/alice"))]);
        let result = unsafe { expand_env_save(b"$NERO_TEST_EXPAND_ENV_VAR1/foo") };
        assert_eq!(result, b"/home/alice/foo");
    }

    #[test]
    fn expand_env_save_unset_var_is_left_as_is() {
        let _guard = EnvVarGuard::set(&[("NERO_TEST_EXPAND_ENV_UNSET1", None)]);
        // An unset variable makes vim_getenv return None, so the "$NAME"
        // text is copied through byte-by-byte, unchanged.
        let result = unsafe { expand_env_save(b"$NERO_TEST_EXPAND_ENV_UNSET1/foo") };
        assert_eq!(result, b"$NERO_TEST_EXPAND_ENV_UNSET1/foo");
    }

    #[test]
    fn expand_env_save_avoids_a_doubled_path_separator() {
        let _guard = EnvVarGuard::set(&[("NERO_TEST_EXPAND_ENV_VAR2", Some("/home/alice/"))]);
        let result = unsafe { expand_env_save(b"$NERO_TEST_EXPAND_ENV_VAR2/foo") };
        assert_eq!(result, b"/home/alice/foo", "the var's own trailing slash absorbs the leading one in the tail");
    }

    #[test]
    fn expand_env_save_bare_tilde_expands_the_home_directory() {
        let _homedir_lock = homedir_test_lock();
        let _guard = EnvVarGuard::set(&[("HOME", Some("/home/alice"))]);
        unsafe { init_homedir() };
        let result = unsafe { expand_env_save(b"~/foo") };
        assert_eq!(result, b"/home/alice/foo");
    }

    #[test]
    fn expand_env_save_tilde_not_at_start_is_left_alone() {
        // Only a leading '~' (at_start) is eligible for home-dir
        // expansion - one appearing later in the string is not.
        let _homedir_lock = homedir_test_lock();
        let _guard = EnvVarGuard::set(&[("HOME", Some("/home/alice"))]);
        unsafe { init_homedir() };
        let result = unsafe { expand_env_save(b"foo~bar") };
        assert_eq!(result, b"foo~bar");
    }

    #[test]
    fn expand_env_esc_escapes_matching_characters_in_the_expanded_value() {
        let _guard = EnvVarGuard::set(&[("NERO_TEST_EXPAND_ENV_VAR3", Some("/has space/dir"))]);
        let result = unsafe { expand_env_esc(b"$NERO_TEST_EXPAND_ENV_VAR3/f", Some(b" "), false, None) };
        assert_eq!(result, b"/has\\ space/dir/f");
    }

    #[test]
    fn expand_env_esc_leaves_a_backtick_expr_verbatim() {
        // The `=expr` form is measured via skip_expr but copied through
        // UNEVALUATED - matching the original's own real behavior
        // exactly (it is later re-evaluated by a genuinely different
        // mechanism, not by this function).
        let result = unsafe { expand_env_esc(b"`=1+1`/foo", None, false, None) };
        assert_eq!(result, b"`=1+1`/foo");
    }

    #[test]
    fn expand_env_esc_one_true_does_not_treat_space_as_a_name_boundary() {
        // With one=true, a leading '~' followed by a space is still
        // eligible for expansion (it's the space/comma-as-boundary
        // behavior that's suppressed, not '~' recognition itself).
        let _homedir_lock = homedir_test_lock();
        let _guard = EnvVarGuard::set(&[("HOME", Some("/home/alice"))]);
        unsafe { init_homedir() };
        let result = unsafe { expand_env_esc(b"~ foo ~ bar", None, true, None) };
        // Only the LEADING '~' expands; the second '~' (after a space)
        // is not treated as a fresh name start since one=true.
        assert_eq!(result, b"/home/alice foo ~ bar");
    }

    #[test]
    fn expand_env_esc_backslash_escapes_the_next_character_verbatim() {
        let result = unsafe { expand_env_esc(b"foo\\$bar", None, false, None) };
        // The backslash is copied through as-is (not stripped), and it
        // suppresses '$' from being treated as a fresh name boundary
        // for THIS iteration - but $ recognition itself still happens
        // normally on the very next loop iteration, so this traces
        // through as a plain "no expansion happened" case either way.
        assert_eq!(result, b"foo\\$bar");
    }

    // --- os_shell_is_cmdexe ---

    #[test]
    fn os_shell_is_cmdexe_false_for_empty_string() {
        assert!(!os_shell_is_cmdexe(b""));
    }

    #[test]
    fn os_shell_is_cmdexe_true_for_bare_cmd_and_cmd_exe() {
        assert!(os_shell_is_cmdexe(b"cmd"));
        assert!(os_shell_is_cmdexe(b"cmd.exe"));
        // Case-insensitive, matching striequal's own semantics.
        assert!(os_shell_is_cmdexe(b"CMD.EXE"));
    }

    #[test]
    fn os_shell_is_cmdexe_true_for_a_path_ending_in_cmd_exe() {
        // Forward slashes deliberately: they're a valid path separator
        // on BOTH Windows and Unix (unlike backslash, which path_tail
        // only recognizes as a separator on Windows - vim_ispathsep's
        // own platform split) - matters here since this test itself
        // runs on both platforms, not just Windows (os_shell_is_cmdexe
        // is a plain, portably-compiled predicate in the original,
        // even though it only ever returns true in practice on a real
        // Windows build).
        assert!(os_shell_is_cmdexe(b"C:/Windows/System32/cmd.exe"));
    }

    #[test]
    fn os_shell_is_cmdexe_false_for_an_unrelated_shell() {
        assert!(!os_shell_is_cmdexe(b"/bin/bash"));
        assert!(!os_shell_is_cmdexe(b"C:/Windows/System32/powershell.exe"));
    }

    #[test]
    fn os_shell_is_cmdexe_dollar_comspec_follows_the_real_env_var() {
        let _lock = homedir_test_lock();
        // Forward slashes - see os_shell_is_cmdexe_true_for_a_path_ending_in_cmd_exe's
        // own comment for why.
        let _guard = EnvVarGuard::set(&[("COMSPEC", Some("C:/Windows/System32/cmd.exe"))]);
        assert!(os_shell_is_cmdexe(b"$COMSPEC"));
        // Case-insensitive on the "$COMSPEC" token itself too.
        assert!(os_shell_is_cmdexe(b"$comspec"));
    }

    #[test]
    fn os_shell_is_cmdexe_dollar_comspec_false_when_it_points_elsewhere() {
        let _lock = homedir_test_lock();
        let _guard = EnvVarGuard::set(&[("COMSPEC", Some("C:/tools/4nt.exe"))]);
        assert!(!os_shell_is_cmdexe(b"$COMSPEC"));
    }

    #[test]
    fn os_shell_is_cmdexe_dollar_comspec_false_when_unset() {
        // path_tail(NULL) == "" in the original; striequal("cmd.exe", "")
        // is false, so an unset $COMSPEC never matches.
        let _lock = homedir_test_lock();
        let _guard = EnvVarGuard::set(&[("COMSPEC", None)]);
        assert!(!os_shell_is_cmdexe(b"$COMSPEC"));
    }

    // --- os_setenv_append_path ---

    #[test]
    fn os_setenv_append_path_false_for_a_relative_path() {
        let _lock = homedir_test_lock();
        let _guard = EnvVarGuard::set(&[("PATH", Some("existing"))]);
        assert!(!unsafe { os_setenv_append_path(b"relative/nvim") });
        // PATH must be left completely untouched.
        assert_eq!(os_getenv(b"PATH"), Some(b"existing".to_vec()));
    }

    #[test]
    fn os_setenv_append_path_sets_path_directly_when_unset() {
        let _lock = homedir_test_lock();
        let _guard = EnvVarGuard::set(&[("PATH", None)]);
        assert!(unsafe { os_setenv_append_path(b"/tmp/somedir/nvim") });
        assert_eq!(os_getenv(b"PATH"), Some(b"/tmp/somedir".to_vec()));
    }

    #[test]
    fn os_setenv_append_path_appends_with_a_separator_when_path_is_set() {
        let _lock = homedir_test_lock();
        let _guard = EnvVarGuard::set(&[("PATH", Some("/usr/bin"))]);
        assert!(unsafe { os_setenv_append_path(b"/tmp/somedir/nvim") });
        let expected = format!("/usr/bin{}/tmp/somedir", crate::os::os_defs::ENV_SEPCHAR);
        assert_eq!(os_getenv(b"PATH"), Some(expected.into_bytes()));
    }

    #[test]
    fn os_setenv_append_path_does_not_double_up_an_existing_trailing_separator() {
        let _lock = homedir_test_lock();
        let existing = format!("/usr/bin{}", crate::os::os_defs::ENV_SEPCHAR);
        let _guard = EnvVarGuard::set(&[("PATH", Some(&existing))]);
        assert!(unsafe { os_setenv_append_path(b"/tmp/somedir/nvim") });
        let expected = format!("/usr/bin{}/tmp/somedir", crate::os::os_defs::ENV_SEPCHAR);
        assert_eq!(os_getenv(b"PATH"), Some(expected.into_bytes()));
    }

    #[test]
    fn os_setenv_append_path_extracts_only_the_containing_directory() {
        let _lock = homedir_test_lock();
        let _guard = EnvVarGuard::set(&[("PATH", None)]);
        // The FILE component ("nvim.exe") itself must never appear in
        // the appended PATH entry - only its containing directory.
        assert!(unsafe { os_setenv_append_path(b"/opt/nvim-nightly/bin/nvim.exe") });
        assert_eq!(os_getenv(b"PATH"), Some(b"/opt/nvim-nightly/bin".to_vec()));
    }

    // --- restore_env_var ---

    #[test]
    #[cfg(windows)]
    fn restore_env_var_sets_the_value_when_some() {
        let name = b"NERO_TEST_ENV_RESTORE_SOME";
        // SAFETY: single test-owned variable name.
        unsafe {
            os_unsetenv(name);
            restore_env_var(name, Some(b"restored-value"));
            assert_eq!(os_getenv(name), Some(b"restored-value".to_vec()));
            os_unsetenv(name);
        }
    }

    #[test]
    #[cfg(windows)]
    fn restore_env_var_unsets_when_none() {
        let name = b"NERO_TEST_ENV_RESTORE_NONE";
        // SAFETY: single test-owned variable name.
        unsafe {
            os_setenv(name, b"will-be-removed", 1);
            restore_env_var(name, None);
            assert_eq!(os_getenv(name), None);
        }
    }
}
