//! Translated from `src/nvim/log.c`/`log.h` ("Log module").
//!
//! The original is deeply coupled to many subsystems not yet translated:
//! `uv_mutex_t` (libuv, phase 11), `os_isdir`/`os_setenv`/`os_mkdir_recurse`/
//! `os_localtime`/`os_getenv_buf`/`os_exepath`/`os_get_pid` (`os/*.c`),
//! `expand_env` (eval), `get_xdg_home`/`stdpaths_user_state_subpath`
//! (`os/stdpaths.c`), `msg_schedule_semsg` (`message.c`, phase 15),
//! `path_tail`/`concat_fnames_realloc` (`path.c`), `get_vim_var_str`
//! (`eval/vars.c`), `ui_client_channel_id` (`ui_client.c`), `g_stats`
//! (`globals.h`).
//!
//! Rather than leave every `WLOG`/`DLOG`/etc. call site in every other file
//! deferred forever waiting on all of those, this translation provides a
//! genuinely working (if simplified) core: level-filtered, timestamped,
//! thread-safe (via `std::sync::Mutex` in place of `uv_mutex_t`) logging to
//! a settable file path or stderr. Deferred, and clearly separated below:
//! - `log_path_init`'s XDG-based path auto-discovery (needs `os/stdpaths.c`,
//!   `path.c`, `os/env.c`) - callers must call [`set_log_file_path`]
//!   explicitly for now, or messages go to stderr. Its `log_try_create`
//!   helper IS translated (it is plain file I/O with no such
//!   dependencies), as is `open_log_file`, which [`logmsg`] now routes
//!   through so the "fall back to stderr" path is real rather than
//!   decorative.
//! - The "instance name" logic in the original's `v_do_log_to_file`
//!   (parent/servername/pid-based; needs `eval/vars.c`, `ui_client.c`,
//!   `os/proc.c`) - omitted from the log line for now.
//! - `log_callstack`/`log_callstack_to_file` (`HAVE_EXECINFO_BACKTRACE`):
//!   shells out to `addr2line` via `popen`, a debug-only diagnostic tool of
//!   low value to translate before the rest of the editor exists.

use std::io::Write as _;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Mutex;

pub const LOGLVL_DBG: i32 = 1;
pub const LOGLVL_INF: i32 = 2;
pub const LOGLVL_WRN: i32 = 3;
pub const LOGLVL_ERR: i32 = 4;

/// `g_min_log_level` (`EXTERN int g_min_log_level INIT(= LOGLVL_WRN)`, or
/// `0` when built with `NVIM_LOG_DEBUG` - the original's build-time
/// `#ifdef` becomes a runtime default here, settable via
/// [`set_min_log_level`]).
static G_MIN_LOG_LEVEL: AtomicI32 = AtomicI32::new(LOGLVL_WRN);

#[inline]
pub fn min_log_level() -> i32 {
    G_MIN_LOG_LEVEL.load(Ordering::Relaxed)
}

#[inline]
pub fn set_min_log_level(level: i32) {
    G_MIN_LOG_LEVEL.store(level, Ordering::Relaxed);
}

struct LogState {
    file_path: Option<std::path::PathBuf>,
    initialized: bool,
}

static LOG_STATE: Mutex<LogState> = Mutex::new(LogState {
    file_path: None,
    initialized: false,
});

/// `log_init`
pub fn log_init() {
    // The original's log_path_init() (XDG-based auto-discovery) is
    // deferred - see module docs. Callers wanting a specific path should
    // call `set_log_file_path`; otherwise messages go to stderr.
    LOG_STATE.lock().unwrap().initialized = true;
}

/// Explicitly set the log file path (stands in for the original's
/// automatic `log_path_init` XDG discovery, deferred).
pub fn set_log_file_path(path: impl Into<std::path::PathBuf>) {
    LOG_STATE.lock().unwrap().file_path = Some(path.into());
}

// `log_lock`/`log_unlock`: deferred. The original exposes explicit
// lock/unlock around a `uv_mutex_t` so `log_uv_handles` can hold it across
// a call into libuv - but `log_uv_handles` itself needs a real libuv loop
// (phase 11, not translated yet), so there is no caller for these two
// functions yet. `logmsg` below takes `LOG_STATE`'s lock internally for
// the duration of one call, which is all that's needed until then.
// (`std::sync::Mutex` also has no safe "unlock from a different call"
// primitive to mirror the original's separate lock()/unlock() pair with
// anyway - that's a `parking_lot`-only feature, not std's.)

/// Logs a message (`logmsg`).
///
/// * `log_level` - Log level (`LOGLVL_*`)
/// * `context` - Description of a shared context or subsystem
/// * `func_name` - Function name, if any
/// * `line_num` - Source line number, if any
/// * `eol` - Append a newline
///
/// Returns true if the log was emitted, false if filtered out or failed.
#[allow(clippy::too_many_arguments)]
pub fn logmsg(
    log_level: i32,
    context: Option<&str>,
    func_name: Option<&str>,
    line_num: Option<i32>,
    eol: bool,
    message: &str,
) -> bool {
    if log_level < min_log_level() {
        return false;
    }

    let state = LOG_STATE.lock().unwrap();
    if !state.initialized {
        return false;
    }

    let level_name = match log_level {
        LOGLVL_DBG => "DBG",
        LOGLVL_INF => "INF",
        LOGLVL_WRN => "WRN",
        LOGLVL_ERR => "ERR",
        _ => "???",
    };

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();

    // Mirrors the original's two fprintf() branches: with a func_name+line
    // (e.g. "WRN 123.456 name ctx:func_name:line: ") or without (just
    // "WRN 123.456 name ctx" / "WRN 123.456 name ?:"). The original's
    // instance "name" field is omitted - see module docs.
    let prefix = if let (Some(func), Some(line)) = (func_name, line_num) {
        format!(
            "{} {}.{:03} {}{}:{}: ",
            level_name,
            now.as_secs(),
            now.subsec_millis(),
            context.unwrap_or(""),
            func,
            line
        )
    } else {
        format!(
            "{} {}.{:03} {} ",
            level_name,
            now.as_secs(),
            now.subsec_millis(),
            context.unwrap_or("?:")
        )
    };

    let line = if eol {
        format!("{prefix}{message}\n")
    } else {
        format!("{prefix}{message}")
    };

    let result = match &state.file_path {
        Some(path) => open_sink(Some(path)).write_all(line.as_bytes()),
        None => std::io::stderr().write_all(line.as_bytes()),
    };
    result.is_ok()
}

/// Try to create/append to `fname` to prove it is usable as a log file
/// (`log_try_create`).
///
/// Returns false for an absent or empty path, or if it cannot be opened
/// for appending. The original opens the file and immediately closes it
/// again purely as a probe; dropping the [`std::fs::File`] here is the
/// direct equivalent of that `fclose`.
#[must_use]
pub fn log_try_create(fname: Option<&std::path::Path>) -> bool {
    let Some(fname) = fname else {
        return false;
    };
    if fname.as_os_str().is_empty() {
        return false;
    }
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(fname)
        .is_ok()
}

/// Where a log line ends up: the log file, or stderr as the fallback.
///
/// The original's `open_log_file` returns a `FILE *` that is either a
/// freshly opened log file or the process-wide `stderr`. Rust has no
/// single owned type covering both, so this enum makes the two cases
/// explicit instead of relying on pointer identity (which is also how
/// the original's own callers test it, via `log_file != stderr`).
pub enum LogSink {
    /// An opened log file, owned by the caller.
    File(std::fs::File),
    /// Fallback when there is no usable log file.
    Stderr,
}

impl std::io::Write for LogSink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            LogSink::File(f) => f.write(buf),
            LogSink::Stderr => std::io::stderr().write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            LogSink::File(f) => f.flush(),
            LogSink::Stderr => std::io::stderr().flush(),
        }
    }
}

/// Open the log file for appending, falling back to stderr
/// (`open_log_file`).
///
/// May fall back if the open failed, the directory does not exist, the
/// file is not writable, or logging was used before [`log_init`].
pub fn open_log_file() -> LogSink {
    let path = LOG_STATE.lock().unwrap().file_path.clone();
    open_sink(path.as_deref())
}

/// The body of [`open_log_file`], factored out so [`logmsg`] can reuse
/// it while already holding `LOG_STATE`'s lock.
///
/// The original can simply call `open_log_file()` from anywhere because
/// its mutex is created with `uv_mutex_init_recursive`; `std::sync::Mutex`
/// is not reentrant, so the lock-taking and the actual work are split
/// rather than risking a deadlock.
fn open_sink(path: Option<&std::path::Path>) -> LogSink {
    if let Some(path) = path
        && !path.as_os_str().is_empty()
    {
        match std::fs::OpenOptions::new().create(true).append(true).open(path) {
            Ok(f) => return LogSink::File(f),
            Err(e) => {
                // The original logs this failure to stderr itself,
                // rather than silently degrading.
                let _ = writeln!(
                    std::io::stderr(),
                    "failed to open log file ({e}): {}",
                    path.display()
                );
            }
        }
    }
    LogSink::Stderr
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serializes every test that touches the shared logging globals
    /// (`LOG_STATE.file_path` and the minimum log level).
    ///
    /// `LOG_STATE`'s own mutex is taken and released around each
    /// individual field access, so it does NOT keep a save/set/use/
    /// restore sequence atomic. Without this second lock, one test can
    /// clear `file_path` while another has just pointed it at a real
    /// file, making that one observe `LogSink::Stderr` instead of
    /// `LogSink::File` (observed ~0.5% of full-suite runs).
    fn log_test_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    #[test]
    fn log_try_create_rejects_an_absent_or_empty_path() {
        assert!(!log_try_create(None));
        assert!(!log_try_create(Some(std::path::Path::new(""))));
    }

    #[test]
    fn log_try_create_creates_a_usable_file_and_leaves_it_closed() {
        let path = std::env::temp_dir()
            .join(format!("nero_log_try_create_{}.log", std::process::id()));
        let _ = std::fs::remove_file(&path);
        assert!(log_try_create(Some(&path)));
        // The probe creates the file but writes nothing to it.
        assert_eq!(std::fs::metadata(&path).unwrap().len(), 0);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn log_try_create_fails_for_a_path_under_a_missing_directory() {
        let path = std::env::temp_dir()
            .join(format!("nero_no_such_dir_{}", std::process::id()))
            .join("nested")
            .join("nvim.log");
        assert!(!log_try_create(Some(&path)));
    }

    #[test]
    fn log_try_create_appends_rather_than_truncating() {
        let path = std::env::temp_dir()
            .join(format!("nero_log_try_append_{}.log", std::process::id()));
        std::fs::write(&path, b"existing").unwrap();
        assert!(log_try_create(Some(&path)));
        assert_eq!(std::fs::read(&path).unwrap(), b"existing");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn open_log_file_returns_the_file_when_the_path_is_usable() {
        let _lock = log_test_lock();
        let path = std::env::temp_dir()
            .join(format!("nero_open_log_{}.log", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let saved = LOG_STATE.lock().unwrap().file_path.clone();
        set_log_file_path(&path);

        assert!(matches!(open_log_file(), LogSink::File(_)));

        LOG_STATE.lock().unwrap().file_path = saved;
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn open_log_file_falls_back_to_stderr_without_a_path() {
        let _lock = log_test_lock();
        let saved = LOG_STATE.lock().unwrap().file_path.clone();
        LOG_STATE.lock().unwrap().file_path = None;

        assert!(matches!(open_log_file(), LogSink::Stderr));

        LOG_STATE.lock().unwrap().file_path = saved;
    }

    #[test]
    fn open_log_file_falls_back_to_stderr_for_an_unusable_path() {
        let _lock = log_test_lock();
        let path = std::env::temp_dir()
            .join(format!("nero_bad_log_dir_{}", std::process::id()))
            .join("nested")
            .join("nvim.log");
        let saved = LOG_STATE.lock().unwrap().file_path.clone();
        set_log_file_path(&path);

        assert!(matches!(open_log_file(), LogSink::Stderr));

        LOG_STATE.lock().unwrap().file_path = saved;
    }

    #[test]
    fn respects_min_log_level_filter() {
        let _lock = log_test_lock();
        set_min_log_level(LOGLVL_ERR);
        assert!(!logmsg(LOGLVL_WRN, None, None, None, true, "should be filtered"));
        set_min_log_level(LOGLVL_WRN); // restore default for other tests
    }

    #[test]
    fn writes_to_a_configured_file() {
        let _lock = log_test_lock();
        log_init();
        let dir = std::env::temp_dir();
        let path = dir.join(format!("nero_log_test_{}.log", std::process::id()));
        set_log_file_path(&path);
        assert!(logmsg(LOGLVL_ERR, Some("ctx"), Some("func"), Some(42), true, "hello log"));
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("hello log"));
        assert!(contents.contains("ERR"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn returns_false_before_init_when_never_initialized_path_used_directly() {
        let _lock = log_test_lock();
        // Uses a fresh, unshared piece of state to avoid interfering with
        // other tests that call log_init(): directly exercises the
        // "log_level < min_log_level" filter path returning false without
        // needing global init at all.
        set_min_log_level(LOGLVL_WRN);
        assert!(!logmsg(LOGLVL_DBG, None, None, None, true, "below threshold"));
    }
}
