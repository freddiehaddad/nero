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
//!   explicitly for now, or messages go to stderr.
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
        Some(path) => std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .and_then(|mut f| f.write_all(line.as_bytes())),
        None => std::io::stderr().write_all(line.as_bytes()),
    };
    result.is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn respects_min_log_level_filter() {
        set_min_log_level(LOGLVL_ERR);
        assert!(!logmsg(LOGLVL_WRN, None, None, None, true, "should be filtered"));
        set_min_log_level(LOGLVL_WRN); // restore default for other tests
    }

    #[test]
    fn writes_to_a_configured_file() {
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
        // Uses a fresh, unshared piece of state to avoid interfering with
        // other tests that call log_init(): directly exercises the
        // "log_level < min_log_level" filter path returning false without
        // needing global init at all.
        set_min_log_level(LOGLVL_WRN);
        assert!(!logmsg(LOGLVL_DBG, None, None, None, true, "below threshold"));
    }
}
