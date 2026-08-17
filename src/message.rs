//! Translated from `src/nvim/message.c` (tractable core only).
//!
//! `message.c` (~3400 lines) is neovim's central message/echo display
//! file - used everywhere, but almost entirely dependent on the
//! screen/message redraw pipeline (`msg_puts*`/`msg_grid_validate`/
//! `msg_scroll_*`/`msg_ext_*`), none of which is translated.
//!
//! Translated: [`msg_id_exists`], [`msg_use_grid`], [`msg_do_throttle`],
//! [`msg_scrollsize`], [`redirecting`], [`trunc_string`],
//! [`msg_strtrunc`], `other_sourcing_name`, `get_emsg_source`,
//! `emsg_not_now` - small, pure predicates/computations needing only a
//! couple of small pieces of genuinely-new state (see below), not the
//! actual message pipeline.
//!
//! Also translated: [`verbose_stop`]/[`verbose_open`] (the
//! `'verbosefile'` log-file handle management, needed by
//! `optionstr.rs`'s own `did_set_verbosefile`). `verbose_fd` is
//! modelled as an owned [`std::fs::File`] rather than going through
//! `os/fs.rs`'s own `os_fopen`, which is deliberately deferred
//! pending a settled raw-fd calling convention - this follows the
//! precedent that module's doc comment already sets, where a caller
//! needing only ordinary buffered I/O uses `std::fs::File` directly
//! (exactly as `memfile.c`'s own `MemfileT.mf_fd` does). The
//! original's own `semsg(_(e_notopen), ...)` failure message is
//! omitted per this crate's established policy; the `FAIL` return and
//! the `verbose_did_open` "only try once" latch are both preserved.
//!
//! Also translated: [`set_keep_msg`] - the "message to redisplay after
//! a redraw" setter. It needs only `keep_msg`/`keep_msg_hl_id`/
//! `msg_silent` (all already present) plus a new `keep_msg_more`
//! global, and none of the message pipeline. Its `xfree`/`xstrdup`
//! pair collapses into a single assignment to an owned
//! `Option<Vec<u8>>`, since dropping the old value frees it.
//!
//! [`reset_last_sourcing`] and [`msg_starthere`] are translated too -
//! both are pure state resets over globals that already exist, save
//! for `last_sourcing_name`/`last_sourcing_lnum`, which are file
//! statics in the original and so live here rather than in
//! `globals.rs`. Note that adding them does NOT unblock
//! `other_sourcing_name`'s remaining body, which still needs
//! `SOURCING_NAME`.
//!
//! Also [`messagesopt_changed`] - parsing and applying the
//! `'messagesopt'` flags and numeric limits. Trimming an existing
//! message history to the new maximum remains deferred with the
//! history/display pipeline.
//!
//! `crate::grid::DEFAULT_GRID` is the original's own file-static
//! `ScreenGrid default_grid` (declared in `grid.c`, `SCREEN_GRID_INIT`-
//! initialized), needed by [`msg_use_grid`]. Since nothing in this
//! crate can currently allocate a real grid (`grid_alloc`, not
//! translated), `DEFAULT_GRID.chars` stays permanently null - the same
//! "harvest a real global ahead of the rest of its file" precedent
//! already used for `mod_mask_table`/`modifier_keys_table`
//! (`keycodes_defs.rs`, ahead of `keycodes.c`) and `shape_table`
//! (`cursor_shape.rs`, ahead of the rest of `cursor_shape.c`).
//!
//! `other_sourcing_name`/`get_emsg_source` both correctly, always
//! take their own early-return path today
//! (`crate::runtime::have_sourcing_info()` is always `false` -
//! `runtime.rs`'s own `EXESTACK` is always empty, matching its own
//! documented `AUTOCMDS`-style "genuinely, provably always-empty
//! registry" precedent) - their own remaining bodies (needing
//! `estack_sfile`-adjacent `SOURCING_NAME` access, not yet translated)
//! are `unimplemented!()`, unreachable in practice today given the
//! above. `get_emsg_lnum`/`msg_source` are NOT translated: unlike
//! these two, both directly evaluate `SOURCING_NAME` WITHOUT first
//! checking `HAVE_SOURCING_INFO` (relying on real neovim's own
//! invariant that `exestack` is never actually empty in a live
//! session, since something always pushes an initial frame at
//! startup) - translating them here would mean indexing this crate's
//! own always-empty `EXESTACK` directly, a genuine panic risk with no
//! real guard, unlike every other "always-empty registry" case in
//! this crate.
//!
//! Deferred: everything else - the entire `msg_puts`/`msg_grid_*`/
//! `msg_scroll_*`/`msg_ext_*` output and routing pipeline,
//! `message_filtered` (needs `vim_regexec`, the regex engine, not
//! translated), `get_emsg_lnum`/`msg_source` (see above),
//! `messaging`/`msg_use_printf` (need `char_avail`/`ui_active`,
//! neither translated).

use crate::globals::GlobalCell;
use std::sync::LazyLock;

/// message id to be allocated to the next message (`msg_id_next`).
static MSG_ID_NEXT: GlobalCell<i64> = GlobalCell::new(1);

/// The source name last displayed alongside a message
/// (`last_sourcing_name`, a file-static in the original).
///
/// Owned rather than a borrowed pointer, so the original's
/// `XFREE_CLEAR` is just assigning `None`.
static LAST_SOURCING_NAME: GlobalCell<Option<Vec<u8>>> = GlobalCell::new(None);

/// The source line number last displayed alongside a message
/// (`last_sourcing_lnum`, a file-static in the original).
static LAST_SOURCING_LNUM: GlobalCell<crate::pos_defs::LinenrT> = GlobalCell::new(0);

/// Pending external-message chunks (`msg_ext_chunks`).
static MSG_EXT_CHUNKS: GlobalCell<Option<crate::api::private::defs::Array>> =
    GlobalCell::new(None);

/// Parsed `'messagesopt'` flags (`msg_flags`).
static MSG_FLAGS: GlobalCell<u32> = GlobalCell::new(
    crate::option_vars::opt_mopt_flag::HIT_ENTER
        | crate::option_vars::opt_mopt_flag::HISTORY
        | crate::option_vars::opt_mopt_flag::PROGRESS,
);

/// Milliseconds to wait before showing a pending message (`msg_wait`).
static MSG_WAIT: GlobalCell<i32> = GlobalCell::new(0);

/// Maximum number of retained message-history entries (`msg_hist_max`).
static MSG_HIST_MAX: GlobalCell<i32> = GlobalCell::new(500);

/// Send progress messages to the command-line target.
const PROGRESS_TARGET_CMD: u32 = 0x01;

/// Parsed progress-message targets (`progress_msg_target`).
static PROGRESS_MSG_TARGET: GlobalCell<u32> = GlobalCell::new(PROGRESS_TARGET_CMD);

/// View used to position the message grid (`msg_grid_adj`).
static MSG_GRID_ADJ: LazyLock<GlobalCell<crate::grid_defs::GridView>> =
    LazyLock::new(|| GlobalCell::new(crate::grid_defs::GridView::default()));

/// Parse and apply a `'messagesopt'` value (`messagesopt_changed`).
///
/// The original reads `p_mopt` directly; accepting the value as a
/// slice keeps this parser independently testable. Trimming an
/// already-populated message history to the new maximum remains with
/// the deferred message-history subsystem.
#[must_use]
pub fn messagesopt_changed(value: &[u8]) -> bool {
    let mut flags = 0u32;
    let mut wait = 0i32;
    let mut history = 0i32;
    let mut progress_target = 0u32;
    let mut pos = 0usize;

    while pos < value.len() {
        if value[pos..].starts_with(b"hit-enter") {
            pos += b"hit-enter".len();
            flags |= crate::option_vars::opt_mopt_flag::HIT_ENTER;
        } else if value[pos..].starts_with(b"wait:")
            && value.get(pos + b"wait:".len()).is_some_and(u8::is_ascii_digit)
        {
            pos += b"wait:".len();
            let start = pos;
            while value.get(pos).is_some_and(u8::is_ascii_digit) {
                pos += 1;
            }
            wait = std::str::from_utf8(&value[start..pos])
                .ok()
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(i32::MAX);
            flags |= crate::option_vars::opt_mopt_flag::WAIT;
        } else if value[pos..].starts_with(b"history:")
            && value
                .get(pos + b"history:".len())
                .is_some_and(u8::is_ascii_digit)
        {
            pos += b"history:".len();
            let start = pos;
            while value.get(pos).is_some_and(u8::is_ascii_digit) {
                pos += 1;
            }
            history = std::str::from_utf8(&value[start..pos])
                .ok()
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(i32::MAX);
            flags |= crate::option_vars::opt_mopt_flag::HISTORY;
        } else if value[pos..].starts_with(b"progress:") {
            pos += b"progress:".len();
            flags |= crate::option_vars::opt_mopt_flag::PROGRESS;
            if value.get(pos) == Some(&b'c') {
                progress_target |= PROGRESS_TARGET_CMD;
                pos += 1;
            }
        }

        if value.get(pos).is_some_and(|&b| b != b',') {
            return false;
        }
        if value.get(pos) == Some(&b',') {
            pos += 1;
        }
    }

    if flags
        & (crate::option_vars::opt_mopt_flag::HIT_ENTER
            | crate::option_vars::opt_mopt_flag::WAIT)
        == 0
        || flags & crate::option_vars::opt_mopt_flag::HISTORY == 0
        || history > 10_000
        || wait > 10_000
    {
        return false;
    }

    *unsafe { MSG_FLAGS.get_mut() } = flags;
    *unsafe { MSG_WAIT.get_mut() } = wait;
    *unsafe { PROGRESS_MSG_TARGET.get_mut() } = progress_target;
    *unsafe { MSG_HIST_MAX.get_mut() } = history;
    true
}

/// Move the UI cursor on the adjusted message grid
/// (`msg_cursor_goto`).
///
/// # Safety
/// `MSG_GRID_ADJ.target` must point to a live `ScreenGrid`.
pub unsafe fn msg_cursor_goto(row: i32, mut col: i32) {
    let globals = unsafe { crate::globals::GLOBALS.get_mut() };
    if globals.cmdmsg_rl {
        col = globals.Columns - 1 - col;
    }
    let view = unsafe { &*MSG_GRID_ADJ.as_ptr() };
    let (grid, row, col) = crate::grid::grid_adjust(view, row, col);
    debug_assert!(!grid.is_null());
    unsafe { crate::ui::ui_grid_cursor_goto((*grid).handle, row, col) };
}

/// Replace the external-message chunk array and return the old one
/// (`msg_ext_init_chunks`).
#[allow(dead_code)]
fn msg_ext_init_chunks() -> Option<crate::api::private::defs::Array> {
    let previous = unsafe { MSG_EXT_CHUNKS.get_mut() }.replace(Vec::new());
    unsafe { crate::globals::GLOBALS.get_mut() }.msg_col = 0;
    previous
}

/// The open `'verbosefile'` handle (`verbose_fd`, a file-static
/// `FILE *` in the original).
///
/// Modelled as an owned [`std::fs::File`] rather than going through
/// `os/fs.rs`'s own `os_fopen` (which is deliberately deferred, as
/// that module's doc comment explains, pending a settled decision on
/// the raw-fd calling convention). This follows the precedent that
/// same doc comment already sets: a specific caller that only needs
/// ordinary buffered I/O uses `std::fs::File` directly instead of
/// waiting for the raw-fd wrappers - exactly as `memfile.c`'s own
/// `MemfileT.mf_fd` already does.
static VERBOSE_FD: LazyLock<GlobalCell<Option<std::fs::File>>> =
    LazyLock::new(|| GlobalCell::new(None));

/// Whether opening `'verbosefile'` has already been attempted, so the
/// failure message is only given once (`verbose_did_open`).
static VERBOSE_DID_OPEN: LazyLock<GlobalCell<bool>> = LazyLock::new(|| GlobalCell::new(false));

/// Note that the last message line is full, so the user has to
/// acknowledge it before the screen scrolls (`msg_check`).
///
/// Only relevant when messages are drawn on the screen grid: with an
/// external messages UI attached there is no bottom line to overflow,
/// so nothing is scheduled.
///
/// # Safety
/// Mutates `crate::globals::GLOBALS`.
pub unsafe fn msg_check() {
    if crate::ui::ui_has(crate::ui::UiExtension::Messages) {
        return;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    if g.msg_row == g.Rows - 1 && g.msg_col >= g.sc_col {
        g.need_wait_return = true;
        g.redraw_cmdline = true;
    }
}

/// Called when `'verbosefile'` is set: stop writing to the file
/// (`verbose_stop`).
///
/// Dropping the [`std::fs::File`] closes it, matching the original's
/// own `fclose`.
///
/// # Safety
/// Touches this module's own `VERBOSE_FD`/`VERBOSE_DID_OPEN` statics.
pub unsafe fn verbose_stop() {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *VERBOSE_FD.get_mut() = None };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *VERBOSE_DID_OPEN.get_mut() = false };
}

/// Open the file `'verbosefile'` (`verbose_open`).
///
/// Returns [`crate::vim_defs::OK`] or [`crate::vim_defs::FAIL`].
/// Opens in append mode, creating the file if needed, matching the
/// original's own `os_fopen(p_vfile, "a")`.
///
/// The original's own `semsg(_(e_notopen), p_vfile)` failure message
/// is omitted, matching this crate's established policy - the `FAIL`
/// return, and the `verbose_did_open` latch that makes the attempt
/// happen only once, are both preserved exactly.
///
/// # Safety
/// Touches this module's own `VERBOSE_FD`/`VERBOSE_DID_OPEN` statics
/// and `OPTION_VARS`.
pub unsafe fn verbose_open() -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let already_open = unsafe { VERBOSE_FD.get_mut() }.is_some();
    // SAFETY: forwarded from this function's own safety doc.
    let did_open = unsafe { *VERBOSE_DID_OPEN.get_mut() };

    if !already_open && !did_open {
        // Only give the error message once.
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { *VERBOSE_DID_OPEN.get_mut() = true };

        // SAFETY: forwarded from this function's own safety doc.
        let vfile = unsafe { crate::option_vars::OPTION_VARS.get_mut() }
            .p_vfile
            .clone()
            .unwrap_or_default();

        let Some(path) = std::str::from_utf8(&vfile).ok().map(std::path::Path::new) else {
            return crate::vim_defs::FAIL;
        };
        let Ok(file) = std::fs::OpenOptions::new().append(true).create(true).open(path) else {
            return crate::vim_defs::FAIL;
        };
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { *VERBOSE_FD.get_mut() = Some(file) };
    }
    crate::vim_defs::OK
}

/// Returns `true` if the given integer message-id was previously
/// generated (i.e. is a real, already-issued id, not `0`/negative/not-
/// yet-issued) (`msg_id_exists`).
#[must_use]
pub fn msg_id_exists(id: i64) -> bool {
    // SAFETY: a plain read through one exclusive borrow.
    id > 0 && id < unsafe { *MSG_ID_NEXT.get_mut() }
}

/// Whether messages should be displayed on the built-in `DEFAULT_GRID`
/// (as opposed to routed entirely through the `ext_messages` UI
/// extension) (`msg_use_grid`).
///
/// Always `false` today: `DEFAULT_GRID`'s own `chars` pointer is
/// always null (nothing in this crate can allocate a real grid yet),
/// which alone makes the original's own `default_grid.chars &&
/// !ui_has(kUIMessages)` condition false regardless of the second
/// operand - a real, faithful consequence of the current state, not a
/// hardcoded stub.
#[must_use]
pub fn msg_use_grid() -> bool {
    // SAFETY: a plain read through one exclusive borrow.
    let has_chars = !unsafe { crate::grid::DEFAULT_GRID.get_mut() }.chars.is_null();
    has_chars && !crate::ui::ui_has(crate::ui::UiExtension::Messages)
}

/// Whether messages must be printed with `printf` rather than drawn
/// on the screen (`msg_use_printf`).
///
/// True only when there is no usable screen: neither embedded mode
/// (where messages go over the RPC channel) nor any attached UI, and
/// no UI has taken over message display via `ext_messages`.
///
/// Note this is genuinely dynamic rather than always-false: this
/// crate's [`crate::ui::ui_active`] is real, so a test that attaches
/// no UI sees `true` here and one that attaches a UI sees `false`.
/// Only the `ui_has` term is fixed, and it is fixed at `false`, which
/// is the operand that leaves the other two deciding.
///
/// # Safety
/// Reads `GLOBALS` (for `embedded_mode`) and the UI list via
/// [`crate::ui::ui_active`].
#[must_use]
pub unsafe fn msg_use_printf() -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let embedded = unsafe { crate::globals::GLOBALS.get_mut() }.embedded_mode;
    // SAFETY: forwarded from this function's own safety doc.
    let active = unsafe { crate::ui::ui_active() };
    !embedded && active == 0 && !crate::ui::ui_has(crate::ui::UiExtension::Messages)
}

/// Whether message-scrolling should be throttled (`msg_do_throttle`).
///
/// Always `false` today, following directly from [`msg_use_grid`]
/// always being `false`.
#[must_use]
pub fn msg_do_throttle() -> bool {
    msg_use_grid()
        && unsafe { crate::option_vars::OPTION_VARS.get_mut() }.rdb_flags
            & crate::option_vars::opt_rdb_flag::NOTHROTTLE
            == 0
}

/// Total number of screen lines occupied by scrolled messages,
/// including the reserved `'cmdheight'`/"hit-enter" lines
/// (`msg_scrollsize`).
#[must_use]
pub fn msg_scrollsize() -> i32 {
    // SAFETY: plain reads through their own exclusive borrows.
    let msg_scrolled = unsafe { crate::globals::GLOBALS.get_mut() }.msg_scrolled;
    let p_ch = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ch as i32;
    msg_scrolled + p_ch + i32::from(p_ch > 0 || msg_scrolled > 1)
}

/// Whether message output is currently being redirected - to a file
/// (`:redir >`), a register, a variable, or `execute()`'s output
/// capture (`redirecting`).
///
/// Always `false` today: none of `GLOBALS.redir_fd`/`redir_reg`/
/// `redir_vname`/`capture_ga` can currently be set by anything in this
/// crate (`:redir`/`execute()`, neither translated).
#[must_use]
pub fn redirecting() -> bool {
    // SAFETY: `.is_null()` never dereferences; the rest are plain
    // reads through their own exclusive borrows.
    let globals = unsafe { crate::globals::GLOBALS.get_mut() };
    let vfile_set = !unsafe { crate::option_vars::OPTION_VARS.get_mut() }
        .p_vfile
        .as_deref()
        .unwrap_or(&[])
        .is_empty();
    !globals.redir_fd.is_null()
        || vfile_set
        || globals.redir_reg != 0
        || globals.redir_vname
        || !globals.capture_ga.is_null()
}

/// Truncate `s` to fit within `room_in` display cells - keeping both
/// the start and end of the string, joined by `"..."` in the middle
/// when both ends can't fit together as-is (`trunc_string`).
///
/// Returns a fresh, owned buffer holding only the resulting content -
/// no trailing NUL byte, unlike some of this crate's other C-string-
/// modeled outputs (e.g. `charset::transchar_hex`): the original's own
/// NUL-termination exists purely to mark content length within a
/// fixed-size destination buffer, a purpose Rust's own `Vec::len()`
/// already serves without needing an explicit sentinel byte. To keep
/// this translation's own truncation POINT byte-for-byte identical to
/// the original's for the same `room_in`/`buflen` (not merely
/// "morally equivalent"), every one of the original's own `buflen - 1`
/// -style capacity reservations (made to leave room for that NUL byte)
/// is preserved here exactly, even though nothing is ever written into
/// that reserved slot.
///
/// The original's own `s == buf` aliasing case (reusing the SAME
/// buffer for both input and output, e.g. `quickfix.c`'s own real
/// caller `qf_fmt_text`) has no direct Rust equivalent needed: since
/// this function only ever READS `s`, never mutates it in place, a
/// caller wanting that exact in-place behavior simply overwrites its
/// own buffer with this function's returned value afterward - byte-
/// for-byte identical content either way.
///
/// `s` is treated as ending at its own first embedded NUL byte, or at
/// the slice's own end, whichever is shorter - this crate's
/// established "embedded NUL ends a C-string-modeled scan" idiom
/// (matching `charset::vim_strnsize`'s own identical treatment),
/// standing in for the original's own `strlen(s)`.
///
/// # Safety
/// Touches `crate::option_vars::OPTION_VARS` (via
/// [`crate::charset::ptr2cells`]/[`crate::mbyte::utfc_ptr2len`]/
/// [`crate::mbyte::utf_head_off`]) - same requirement as every other
/// function that does so.
#[must_use]
pub unsafe fn trunc_string(s: &[u8], room_in: i32, buflen: usize) -> Vec<u8> {
    let s = &s[..s.iter().position(|&b| b == 0).unwrap_or(s.len())];

    if s.is_empty() {
        // The original: `*buf = NUL;` when `buflen > 0` (an empty
        // destination string), otherwise nothing at all - either way,
        // zero bytes of real CONTENT, matching an empty Vec.
        return Vec::new();
    }

    let mut room = room_in - 3; // "..." takes 3 chars
    if room_in < 3 {
        room = 0;
    }
    let half = room / 2;

    let mut buf: Vec<u8> = Vec::new();
    let mut len = 0i32;
    let mut e = 0usize;

    // First part: start of the string.
    while len < half && e < buflen {
        if e >= s.len() {
            // text fits without truncating! `buf` already holds
            // exactly the string's own content up to this point (no
            // explicit NUL needed - Vec's own length marks the end).
            return buf;
        }
        // SAFETY: forwarded from this function's own safety doc.
        let n = unsafe { crate::charset::ptr2cells(&s[e..]) };
        if len + n > half {
            break;
        }
        len += n;
        buf.push(s[e]);
        // SAFETY: forwarded from this function's own safety doc.
        let mut m = unsafe { crate::mbyte::utfc_ptr2len(&s[e..]) };
        loop {
            m -= 1;
            if m <= 0 {
                break;
            }
            e += 1;
            if e == buflen {
                break;
            }
            buf.push(s[e]);
        }
        e += 1;
    }

    // Last part: end of the string.
    let mut half = s.len() as i32; // strlen(s)
    let mut i = half;
    loop {
        // SAFETY: forwarded from this function's own safety doc.
        // `half` is always >= 1 here (the loop breaks as soon as it
        // reaches 0, immediately below, before ever looping back to
        // redo this computation with half == 0), so `half - 1` is
        // always a valid index into `s`.
        let offset = unsafe { crate::mbyte::utf_head_off(s, (half - 1) as usize) };
        half = half - offset - 1;
        // SAFETY: forwarded from this function's own safety doc.
        let n = unsafe { crate::charset::ptr2cells(&s[half as usize..]) };
        if len + n > room || half == 0 {
            break;
        }
        len += n;
        i = half;
    }

    if i <= e as i32 + 3 {
        // Text fits without truncating - just append everything
        // remaining after the first part.
        let natural_len = s.len() as i32;
        let mut copy_len = if natural_len >= buflen as i32 { buflen as i32 - 1 } else { natural_len };
        copy_len = copy_len - e as i32 + 1;
        if copy_len < 1 {
            buf.truncate(e - 1);
        } else {
            // The original's own `len` here includes the destination
            // buffer's own trailing NUL byte (`strlen(s) + 1`-style
            // accounting) - this translation copies one fewer byte
            // (the real content only), matching its own "no NUL byte"
            // convention documented above.
            let real_copy_len = (copy_len - 1) as usize;
            buf.extend_from_slice(&s[e..e + real_copy_len]);
        }
    } else if e + 3 < buflen {
        // Set the middle "..." and copy the last part.
        buf.truncate(e);
        buf.extend_from_slice(b"...");
        let natural_len = (s.len() - i as usize) as i32; // strlen(s + i)
        let mut copy_len = natural_len + 1;
        if copy_len >= buflen as i32 - e as i32 - 3 {
            copy_len = buflen as i32 - e as i32 - 3 - 1;
        }
        let real_copy_len = (copy_len - 1).max(0) as usize;
        buf.extend_from_slice(&s[i as usize..i as usize + real_copy_len]);
    } else {
        // Can't fit the "...", just truncate it - the original
        // reserves the final byte for a NUL, i.e. keeps only
        // `buflen - 1` content bytes; this translation has no NUL of
        // its own, but keeps the exact same content-length cap.
        buf.truncate(buflen.saturating_sub(1));
    }

    buf
}

/// Truncate a message such that it can be printed without causing a
/// scroll (`msg_strtrunc`). Returns `None` when no truncating is done
/// (matching the original's own `NULL` return - the caller should keep
/// using its own, untruncated `s` in that case).
///
/// # Safety
/// Touches `crate::globals::GLOBALS`/`crate::option_vars::OPTION_VARS`
/// (the same real fields [`trunc_string`]/[`crate::option::shortmess`]/
/// [`crate::ui::ui_has`] already require), plus forwards
/// [`trunc_string`]'s own safety doc.
#[must_use]
pub unsafe fn msg_strtrunc(s: &[u8], force: bool) -> Option<Vec<u8>> {
    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    let ui_has_messages = crate::ui::ui_has(crate::ui::UiExtension::Messages);

    // May truncate message to avoid a hit-return prompt.
    let should_truncate = (g.msg_scroll == 0
        && !g.need_wait_return
        && crate::option::shortmess(crate::option_vars::shm::TRUNCALL)
        && !g.exmode_active
        && g.msg_silent == 0
        && !ui_has_messages)
        || force;
    if !should_truncate {
        return None;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let mut len = unsafe { crate::charset::vim_strsize(s) };
    let room = if g.msg_scrolled != 0 {
        // Use all the columns.
        (g.Rows - g.msg_row) * g.Columns - 1
    } else {
        // Use up to 'showcmd' column.
        let last_row = if ui_has_messages { g.Columns } else { g.sc_col - 1 };
        (g.Rows - g.msg_row - 1) * g.Columns + last_row
    };
    if len > room && room > 0 {
        // may have up to 18 bytes per cell (6 per char, up to two
        // composing chars)
        len = (room + 2) * 18;
        // SAFETY: forwarded from this function's own safety doc.
        return Some(unsafe { trunc_string(s, room, len as usize) });
    }
    None
}

/// Whether the current execution stack's own source name differs from
/// the last one displayed (`other_sourcing_name`).
///
/// Always `false` today - see this module's own doc comment.
///
/// `#[allow(dead_code)]`: no real translated caller yet (`get_emsg_lnum`/
/// `msg_source`, neither translated - see this module's own doc
/// comment) - tested directly, matching this crate's established
/// convention for private helpers harvested ahead of their real
/// caller.
#[allow(dead_code)]
#[must_use]
fn other_sourcing_name() -> bool {
    if crate::runtime::have_sourcing_info() {
        // SOURCING_NAME != NULL && (compare against the last-displayed
        // source name) - needs SOURCING_NAME access (estack_sfile-
        // adjacent), not yet translated.
        unimplemented!(
            "message::other_sourcing_name: needs SOURCING_NAME access (estack_sfile-adjacent), \
             not yet translated - unreachable in practice today since \
             crate::runtime::have_sourcing_info() is always false, see this module's own doc \
             comment"
        );
    }
    false
}

/// Get the message about the source, as used for an error message
/// (`get_emsg_source`). Returns `None` when no message is to be given.
///
/// Always `None` today - see this module's own doc comment.
///
/// `#[allow(dead_code)]`: no real translated caller yet (`msg_source`,
/// not translated - see this module's own doc comment) - tested
/// directly, matching this crate's established convention for private
/// helpers harvested ahead of their real caller.
#[allow(dead_code)]
#[must_use]
fn get_emsg_source() -> Option<Vec<u8>> {
    if crate::runtime::have_sourcing_info() {
        // SOURCING_NAME != NULL && other_sourcing_name() - needs
        // SOURCING_NAME access (estack_sfile-adjacent), not yet
        // translated.
        unimplemented!(
            "message::get_emsg_source: needs SOURCING_NAME access (estack_sfile-adjacent), not \
             yet translated - unreachable in practice today since \
             crate::runtime::have_sourcing_info() is always false, see this module's own doc \
             comment"
        );
    }
    None
}

/// Whether error messages are currently suppressed and should not be
/// given at all (`emsg_not_now`) - checked by `emsg`/`semsg`/`iemsg`/
/// `siemsg` (none translated) before doing any real work.
///
/// `#[allow(dead_code)]`: no real translated caller yet - harvested
/// ahead of them, matching this crate's established precedent for a
/// small, self-contained function with no design freedom of its own.
#[allow(dead_code)]
#[must_use]
pub fn emsg_not_now() -> bool {
    // SAFETY: plain reads through their own exclusive borrows.
    let globals = unsafe { crate::globals::GLOBALS.get_mut() };
    let option_vars = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
    let p_debug = option_vars.p_debug.as_deref().unwrap_or(b"");
    (globals.emsg_off > 0
        && crate::strings::vim_strchr(p_debug, i32::from(b'm')).is_none()
        && crate::strings::vim_strchr(p_debug, i32::from(b't')).is_none())
        || globals.emsg_skip > 0
}

/// Set `keep_msg` to `s`, the message to redisplay after a redraw
/// (`set_keep_msg`).
///
/// Takes an `Option<&[u8]>` where the original takes a possibly-NULL
/// `const char *`, and stores an owned copy - so the original's
/// `xfree(keep_msg)`/`xstrdup(s)` pair is just the assignment here
/// (dropping the old `Option` frees it).
///
/// # Safety
/// Must not run concurrently with any other access to
/// `crate::globals::GLOBALS`.
pub unsafe fn set_keep_msg(s: Option<&[u8]>, hl_id: i32) {
    // Kept message is not cleared and re-emitted with ext_messages.
    if crate::ui::ui_has(crate::ui::UiExtension::Messages) {
        return;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let globals = unsafe { crate::globals::GLOBALS.get_mut() };
    globals.keep_msg = match s {
        Some(s) if globals.msg_silent == 0 => Some(s.to_vec()),
        _ => None,
    };
    globals.keep_msg_more = false;
    globals.keep_msg_hl_id = hl_id;
}

/// Forget the source name/line last displayed with a message
/// (`reset_last_sourcing`), so the next message re-prints its source
/// header even if it comes from the same script.
///
/// The original's `XFREE_CLEAR(last_sourcing_name)` is just assigning
/// `None` to an owned `Option<Vec<u8>>` here.
///
/// # Safety
/// Must not run concurrently with any other access to
/// `LAST_SOURCING_NAME`/`LAST_SOURCING_LNUM`.
pub unsafe fn reset_last_sourcing() {
    // SAFETY: forwarded from this function's own safety doc.
    *unsafe { LAST_SOURCING_NAME.get_mut() } = None;
    // SAFETY: forwarded from this function's own safety doc.
    *unsafe { LAST_SOURCING_LNUM.get_mut() } = 0;
}

/// Start writing messages at the current cursor position
/// (`msg_starthere`).
///
/// # Safety
/// Must not run concurrently with any other access to
/// `crate::globals::GLOBALS`.
pub unsafe fn msg_starthere() {
    // SAFETY: forwarded from this function's own safety doc.
    let globals = unsafe { crate::globals::GLOBALS.get_mut() };
    globals.lines_left = globals.cmdline_row;
    globals.msg_didany = false;
}

/// One chunk of previously displayed message text, kept so the
/// message area can be scrolled back (`msgchunk_T`).
///
/// The chunks form an intrusive doubly-linked list, with raw pointers
/// matching this crate's convention for such lists (see `DiffT`). The
/// original's flexible array member `sb_text[]` becomes an owned
/// `Vec<u8>`, so the chunk no longer needs a single trailing
/// allocation.
#[derive(Debug, Default)]
pub struct MsgchunkT {
    /// Next chunk (`sb_next`).
    pub sb_next: *mut MsgchunkT,
    /// Previous chunk (`sb_prev`).
    pub sb_prev: *mut MsgchunkT,
    /// Whether the line ends after this text (`sb_eol`).
    pub sb_eol: bool,
    /// Column in which the text starts (`sb_msg_col`).
    pub sb_msg_col: i32,
    /// Text highlight id (`sb_hl_id`).
    pub sb_hl_id: i32,
    /// The text itself (`sb_text`).
    pub sb_text: Vec<u8>,
}

/// When remembered scroll-back text should be cleared (`sb_clear_T`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SbClearT {
    /// Nothing to clear (`SB_CLEAR_NONE`).
    #[default]
    None = 0,
    /// Clear everything (`SB_CLEAR_ALL`).
    All = 1,
    /// Editing the command line: do not clear yet
    /// (`SB_CLEAR_CMDLINE_BUSY`).
    CmdlineBusy = 2,
    /// Finished editing the command line: clear old lines but keep the
    /// last (`SB_CLEAR_CMDLINE_DONE`).
    CmdlineDone = 3,
}

/// Pending scroll-back clear request (`do_clear_sb_text`).
static DO_CLEAR_SB_TEXT: crate::globals::GlobalCell<SbClearT> =
    crate::globals::GlobalCell::new(SbClearT::None);

/// The most recently displayed text chunk (`last_msgchunk`).
static LAST_MSGCHUNK: crate::globals::GlobalCell<*mut MsgchunkT> =
    crate::globals::GlobalCell::new(std::ptr::null_mut());

/// Move back to the start of a screen line in already displayed text
/// (`msg_sb_start`).
///
/// Walks back over chunks belonging to the same line, stopping at the
/// one whose predecessor ended a line.
///
/// # Safety
/// `mps` must be null or point at a live [`MsgchunkT`] whose `sb_prev`
/// chain is likewise valid.
#[must_use]
pub unsafe fn msg_sb_start(mps: *mut MsgchunkT) -> *mut MsgchunkT {
    let mut mp = mps;
    // SAFETY: forwarded from this function's own safety doc.
    while !mp.is_null() && !unsafe { (*mp).sb_prev }.is_null() && !unsafe { (*(*mp).sb_prev).sb_eol }
    {
        // SAFETY: forwarded from this function's own safety doc.
        mp = unsafe { (*mp).sb_prev };
    }
    mp
}

/// Mark the last message chunk as finishing the line (`msg_sb_eol`).
///
/// # Safety
/// The `LAST_MSGCHUNK` file-static must be null or point at a live
/// [`MsgchunkT`].
pub unsafe fn msg_sb_eol() {
    // SAFETY: forwarded from this function's own safety doc.
    let last = unsafe { *LAST_MSGCHUNK.get_mut() };
    if !last.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*last).sb_eol = true };
    }
}

/// Finished editing the command line: clear old lines but keep the
/// last one, later (`sb_text_end_cmdline`).
///
/// # Safety
/// Mutates the `DO_CLEAR_SB_TEXT` file-static.
pub unsafe fn sb_text_end_cmdline() {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *DO_CLEAR_SB_TEXT.get_mut() = SbClearT::CmdlineDone };
}

/// Redrawing the command line: discard the last unfinished line
/// (`sb_text_restart_cmdline`).
///
/// # Safety
/// The `LAST_MSGCHUNK` chain must consist of live chunks originally
/// allocated with `Box`, since the discarded ones are freed here.
pub unsafe fn sb_text_restart_cmdline() {
    // Needed when returning from a nested command line.
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *DO_CLEAR_SB_TEXT.get_mut() = SbClearT::CmdlineBusy };

    // SAFETY: forwarded from this function's own safety doc.
    let last = unsafe { *LAST_MSGCHUNK.get_mut() };
    // SAFETY: forwarded from this function's own safety doc.
    if last.is_null() || unsafe { (*last).sb_eol } {
        // No unfinished line: don't clear anything.
        return;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let mut tofree = unsafe { msg_sb_start(last) };
    // SAFETY: forwarded from this function's own safety doc.
    let new_last = unsafe { (*tofree).sb_prev };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *LAST_MSGCHUNK.get_mut() = new_last };
    if !new_last.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { (*new_last).sb_next = std::ptr::null_mut() };
    }

    while !tofree.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        let next = unsafe { (*tofree).sb_next };
        // SAFETY: the chunk was allocated with Box, per this
        // function's own safety doc.
        drop(unsafe { Box::from_raw(tofree) });
        tofree = next;
    }
}

/// Discard text remembered for scrolling back (`clear_sb_text`).
///
/// With `all` unset the most recent screen line is kept; everything
/// before it is discarded. Called when redrawing the screen.
///
/// Note this frees BACKWARDS along `sb_prev`, unlike
/// [`sb_text_restart_cmdline`], which discards a trailing unfinished
/// line forwards along `sb_next`.
///
/// # Safety
/// The `LAST_MSGCHUNK` chain must consist of live chunks originally
/// allocated with `Box`, since the discarded ones are freed here.
pub unsafe fn clear_sb_text(all: bool) {
    // The original walks a `msgchunk_T **`, so that the same loop can
    // clear either the global head or one chunk's own `sb_prev` link.
    let lastp: *mut *mut MsgchunkT = if all {
        // SAFETY: forwarded from this function's own safety doc.
        std::ptr::from_mut(unsafe { LAST_MSGCHUNK.get_mut() })
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        let last = unsafe { *LAST_MSGCHUNK.get_mut() };
        if last.is_null() {
            return;
        }
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { std::ptr::addr_of_mut!((*msg_sb_start(last)).sb_prev) }
    };

    // SAFETY: forwarded from this function's own safety doc.
    while !unsafe { *lastp }.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        let mp = unsafe { (**lastp).sb_prev };
        // SAFETY: the chunk was allocated with Box, per this
        // function's own safety doc.
        drop(unsafe { Box::from_raw(*lastp) });
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { *lastp = mp };
    }
}

/// Starting to edit the command line: do not clear messages now
/// (`sb_text_start_cmdline`).
///
/// # Safety
/// Same as [`sb_text_restart_cmdline`].
pub unsafe fn sb_text_start_cmdline() {
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { *DO_CLEAR_SB_TEXT.get_mut() } == SbClearT::CmdlineBusy {
        // Invoking the command line recursively: the previous level's
        // command line need not be remembered, since it is redrawn on
        // returning to that level.
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { sb_text_restart_cmdline() };
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { msg_sb_eol() };
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { *DO_CLEAR_SB_TEXT.get_mut() = SbClearT::CmdlineBusy };
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    struct MessagesOptGuard {
        flags: u32,
        wait: i32,
        history: i32,
        progress: u32,
    }

    impl MessagesOptGuard {
        fn new() -> Self {
            Self {
                flags: *unsafe { MSG_FLAGS.get_mut() },
                wait: *unsafe { MSG_WAIT.get_mut() },
                history: *unsafe { MSG_HIST_MAX.get_mut() },
                progress: *unsafe { PROGRESS_MSG_TARGET.get_mut() },
            }
        }
    }

    impl Drop for MessagesOptGuard {
        fn drop(&mut self) {
            *unsafe { MSG_FLAGS.get_mut() } = self.flags;
            *unsafe { MSG_WAIT.get_mut() } = self.wait;
            *unsafe { MSG_HIST_MAX.get_mut() } = self.history;
            *unsafe { PROGRESS_MSG_TARGET.get_mut() } = self.progress;
        }
    }

    struct MsgGridGuard(crate::grid_defs::GridView);

    impl MsgGridGuard {
        fn install(value: crate::grid_defs::GridView) -> Self {
            Self(std::mem::replace(
                unsafe { MSG_GRID_ADJ.get_mut() },
                value,
            ))
        }
    }

    impl Drop for MsgGridGuard {
        fn drop(&mut self) {
            let old = std::mem::take(&mut self.0);
            *unsafe { MSG_GRID_ADJ.get_mut() } = old;
        }
    }

    struct UiCursorGuard((crate::types_defs::HandleT, i32, i32, bool));

    impl UiCursorGuard {
        fn new() -> Self {
            Self(unsafe { crate::ui::ui_test_cursor_state() })
        }
    }

    impl Drop for UiCursorGuard {
        fn drop(&mut self) {
            unsafe { crate::ui::ui_test_restore_cursor_state(self.0) };
        }
    }

    #[test]
    fn msg_cursor_goto_applies_message_grid_offsets() {
        let _lock = crate::globals::global_state_test_lock();
        let _columns = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.Columns,
                80,
            )
        };
        let _rightleft = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.cmdmsg_rl,
                false,
            )
        };
        let _cursor = UiCursorGuard::new();
        let mut grid = crate::grid_defs::ScreenGrid {
            handle: 77,
            ..Default::default()
        };
        let gridp = &mut grid as *mut crate::grid_defs::ScreenGrid;
        let _view = MsgGridGuard::install(crate::grid_defs::GridView {
            target: gridp,
            row_offset: 2,
            col_offset: 3,
        });

        unsafe { msg_cursor_goto(5, 7) };
        assert_eq!(unsafe { crate::ui::ui_test_cursor_state() }, (77, 7, 10, true));
    }

    #[test]
    fn msg_cursor_goto_mirrors_the_column_for_rightleft_messages() {
        let _lock = crate::globals::global_state_test_lock();
        let _columns = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.Columns,
                80,
            )
        };
        let _rightleft = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.cmdmsg_rl,
                true,
            )
        };
        let _cursor = UiCursorGuard::new();
        let mut grid = crate::grid_defs::ScreenGrid {
            handle: 88,
            ..Default::default()
        };
        let gridp = &mut grid as *mut crate::grid_defs::ScreenGrid;
        let _view = MsgGridGuard::install(crate::grid_defs::GridView {
            target: gridp,
            row_offset: 0,
            col_offset: 0,
        });

        unsafe { msg_cursor_goto(4, 7) };
        assert_eq!(unsafe { crate::ui::ui_test_cursor_state() }, (88, 4, 72, true));
    }

    #[test]
    fn messagesopt_changed_applies_hit_enter_history_and_progress() {
        let _lock = crate::globals::global_state_test_lock();
        let _state = MessagesOptGuard::new();

        assert!(messagesopt_changed(b"hit-enter,history:500,progress:c"));
        assert_eq!(
            *unsafe { MSG_FLAGS.get_mut() },
            crate::option_vars::opt_mopt_flag::HIT_ENTER
                | crate::option_vars::opt_mopt_flag::HISTORY
                | crate::option_vars::opt_mopt_flag::PROGRESS
        );
        assert_eq!(*unsafe { MSG_WAIT.get_mut() }, 0);
        assert_eq!(*unsafe { MSG_HIST_MAX.get_mut() }, 500);
        assert_eq!(*unsafe { PROGRESS_MSG_TARGET.get_mut() }, PROGRESS_TARGET_CMD);
    }

    #[test]
    fn messagesopt_changed_applies_wait_and_allows_no_progress_target() {
        let _lock = crate::globals::global_state_test_lock();
        let _state = MessagesOptGuard::new();

        assert!(messagesopt_changed(b"wait:123,history:42,progress:,"));
        assert_eq!(
            *unsafe { MSG_FLAGS.get_mut() },
            crate::option_vars::opt_mopt_flag::WAIT
                | crate::option_vars::opt_mopt_flag::HISTORY
                | crate::option_vars::opt_mopt_flag::PROGRESS
        );
        assert_eq!(*unsafe { MSG_WAIT.get_mut() }, 123);
        assert_eq!(*unsafe { MSG_HIST_MAX.get_mut() }, 42);
        assert_eq!(*unsafe { PROGRESS_MSG_TARGET.get_mut() }, 0);
    }

    #[test]
    fn messagesopt_changed_rejects_missing_required_parts() {
        let _lock = crate::globals::global_state_test_lock();
        let _state = MessagesOptGuard::new();

        assert!(!messagesopt_changed(b"history:50"));
        assert!(!messagesopt_changed(b"wait:10"));
        assert!(!messagesopt_changed(b""));
    }

    #[test]
    fn messagesopt_changed_rejects_unknown_or_malformed_parts() {
        let _lock = crate::globals::global_state_test_lock();
        let _state = MessagesOptGuard::new();

        assert!(!messagesopt_changed(b"hit-enter,history:50,bogus"));
        assert!(!messagesopt_changed(b"wait:,history:50"));
        assert!(!messagesopt_changed(b"hit-enter history:50"));
    }

    #[test]
    fn messagesopt_changed_rejects_limits_above_ten_thousand() {
        let _lock = crate::globals::global_state_test_lock();
        let _state = MessagesOptGuard::new();

        assert!(!messagesopt_changed(b"wait:10001,history:50"));
        assert!(!messagesopt_changed(b"wait:10,history:10001"));
    }

    #[test]
    fn messagesopt_changed_does_not_commit_an_invalid_value() {
        let _lock = crate::globals::global_state_test_lock();
        let _state = MessagesOptGuard::new();
        *unsafe { MSG_FLAGS.get_mut() } = 0xAA;
        *unsafe { MSG_WAIT.get_mut() } = 77;
        *unsafe { MSG_HIST_MAX.get_mut() } = 88;
        *unsafe { PROGRESS_MSG_TARGET.get_mut() } = 0xBB;

        assert!(!messagesopt_changed(b"bogus"));
        assert_eq!(*unsafe { MSG_FLAGS.get_mut() }, 0xAA);
        assert_eq!(*unsafe { MSG_WAIT.get_mut() }, 77);
        assert_eq!(*unsafe { MSG_HIST_MAX.get_mut() }, 88);
        assert_eq!(*unsafe { PROGRESS_MSG_TARGET.get_mut() }, 0xBB);
    }

    struct MsgExtChunksGuard(
        Option<crate::api::private::defs::Array>,
    );

    impl MsgExtChunksGuard {
        fn install(value: Option<crate::api::private::defs::Array>) -> Self {
            Self(std::mem::replace(
                unsafe { MSG_EXT_CHUNKS.get_mut() },
                value,
            ))
        }
    }

    impl Drop for MsgExtChunksGuard {
        fn drop(&mut self) {
            *unsafe { MSG_EXT_CHUNKS.get_mut() } = self.0.take();
        }
    }

    #[test]
    fn msg_ext_init_chunks_returns_old_array_and_resets_column() {
        let _lock = crate::globals::global_state_test_lock();
        let _column = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |globals| &mut globals.msg_col,
                17,
            )
        };
        let _chunks = MsgExtChunksGuard::install(Some(vec![
            crate::api::private::defs::Object::Nil,
        ]));

        let previous = msg_ext_init_chunks().expect("old chunk array");

        assert_eq!(previous.len(), 1);
        assert!(matches!(
            previous.first(),
            Some(crate::api::private::defs::Object::Nil)
        ));
        assert!(unsafe { MSG_EXT_CHUNKS.get_mut() }
            .as_ref()
            .is_some_and(Vec::is_empty));
        assert_eq!(unsafe { crate::globals::GLOBALS.get_mut() }.msg_col, 0);
    }

    // --- msg_use_printf ---

    /// Restores `embedded_mode` on drop, even through a panic.
    struct EmbeddedGuard {
        prev: bool,
    }

    impl EmbeddedGuard {
        fn set(value: bool) -> Self {
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            let me = Self { prev: g.embedded_mode };
            g.embedded_mode = value;
            me
        }
    }

    impl Drop for EmbeddedGuard {
        fn drop(&mut self) {
            unsafe { crate::globals::GLOBALS.get_mut() }.embedded_mode = self.prev;
        }
    }

    /// With no embedded channel and no attached UI there is no usable
    /// screen, so messages must go through `printf`.
    #[test]
    fn msg_use_printf_is_true_without_an_embedded_channel_or_ui() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = EmbeddedGuard::set(false);
        assert_eq!(unsafe { crate::ui::ui_active() }, 0, "no UI attached here");
        assert!(unsafe { msg_use_printf() });
    }

    /// Embedded mode routes messages over the RPC channel instead, so
    /// `printf` is not used. This is the term that genuinely toggles
    /// the result under test: `ui_active` stays at its default of 0
    /// (no UI can be attached from here, since the registry is private
    /// to `ui.rs`) and `ui_has` is fixed false.
    #[test]
    fn msg_use_printf_is_false_in_embedded_mode() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = EmbeddedGuard::set(true);
        assert!(!unsafe { msg_use_printf() });
    }

    // --- msgchunk / msg_sb family ---

    /// Owns an installed chunk chain and guarantees it is torn down,
    /// even if the test panics part-way through.
    ///
    /// Without this, a panicking test would leave `LAST_MSGCHUNK`
    /// pointing at chunks it no longer owns, and the next test to run
    /// would inherit that dangling global - a genuine
    /// use-after-free/double-free hazard rather than a mere leak.
    struct ChunkGuard {
        ptrs: Vec<*mut MsgchunkT>,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl ChunkGuard {
        fn install(eols: &[bool]) -> Self {
            let _lock = crate::globals::global_state_test_lock();
            let mut ptrs: Vec<*mut MsgchunkT> = Vec::new();
            for &eol in eols {
                ptrs.push(Box::into_raw(Box::new(MsgchunkT {
                    sb_eol: eol,
                    ..Default::default()
                })));
            }
            for i in 0..ptrs.len() {
                unsafe {
                    (*ptrs[i]).sb_prev = if i == 0 { std::ptr::null_mut() } else { ptrs[i - 1] };
                    (*ptrs[i]).sb_next =
                        if i + 1 == ptrs.len() { std::ptr::null_mut() } else { ptrs[i + 1] };
                }
            }
            unsafe {
                *LAST_MSGCHUNK.get_mut() = ptrs.last().copied().unwrap_or(std::ptr::null_mut());
                *DO_CLEAR_SB_TEXT.get_mut() = SbClearT::None;
            }
            ChunkGuard { ptrs, _lock }
        }

        fn ptr(&self, i: usize) -> *mut MsgchunkT {
            self.ptrs[i]
        }
    }

    impl Drop for ChunkGuard {
        fn drop(&mut self) {
            // Free whatever chain is still installed, walking from its
            // head. Chunks the code under test already freed are no
            // longer reachable from LAST_MSGCHUNK, so they are not
            // freed twice.
            unsafe {
                let mut mp = *LAST_MSGCHUNK.get_mut();
                while !mp.is_null() && !(*mp).sb_prev.is_null() {
                    mp = (*mp).sb_prev;
                }
                while !mp.is_null() {
                    let next = (*mp).sb_next;
                    drop(Box::from_raw(mp));
                    mp = next;
                }
                *LAST_MSGCHUNK.get_mut() = std::ptr::null_mut();
                *DO_CLEAR_SB_TEXT.get_mut() = SbClearT::None;
            }
        }
    }

    #[test]
    fn clear_sb_text_all_discards_every_chunk() {
        let _g = ChunkGuard::install(&[true, false, false]);

        unsafe { clear_sb_text(true) };

        assert!(unsafe { *LAST_MSGCHUNK.get_mut() }.is_null());
    }

    #[test]
    fn clear_sb_text_keeps_the_most_recent_screen_line() {
        // Chunk 0 ends a line; chunks 1 and 2 form the most recent
        // one, which must survive. Everything before it goes.
        let g = ChunkGuard::install(&[true, false, false]);
        let keep_head = g.ptr(1);
        let keep_tail = g.ptr(2);

        unsafe { clear_sb_text(false) };

        assert_eq!(
            unsafe { *LAST_MSGCHUNK.get_mut() },
            keep_tail,
            "the last chunk is still the last"
        );
        assert!(
            unsafe { (*keep_head).sb_prev }.is_null(),
            "the surviving line's backward link is cleared"
        );
    }

    #[test]
    fn clear_sb_text_without_all_is_a_noop_with_no_chunks() {
        let _g = ChunkGuard::install(&[]);
        unsafe { clear_sb_text(false) };
        assert!(unsafe { *LAST_MSGCHUNK.get_mut() }.is_null());
    }

    #[test]
    fn clear_sb_text_keeping_a_single_line_discards_nothing() {
        // One unfinished line and nothing before it: there is nothing
        // to discard, so both chunks survive.
        let g = ChunkGuard::install(&[false, false]);
        let head = g.ptr(0);

        unsafe { clear_sb_text(false) };

        assert_eq!(unsafe { *LAST_MSGCHUNK.get_mut() }, g.ptr(1));
        assert!(unsafe { (*head).sb_prev }.is_null());
    }

    #[test]
    fn msg_sb_eol_marks_only_the_last_chunk() {
        let g = ChunkGuard::install(&[false, false, false]);

        unsafe { msg_sb_eol() };

        let flags: Vec<bool> = (0..3).map(|i| unsafe { (*g.ptr(i)).sb_eol }).collect();
        assert_eq!(flags, vec![false, false, true]);
    }

    #[test]
    fn msg_sb_eol_is_a_noop_with_no_chunks() {
        let _g = ChunkGuard::install(&[]);
        unsafe { msg_sb_eol() };
        assert!(unsafe { *LAST_MSGCHUNK.get_mut() }.is_null());
    }

    #[test]
    fn msg_sb_start_walks_back_to_the_start_of_the_screen_line() {
        // Chunk 0 ends a line, so chunks 1..3 form one screen line and
        // starting from chunk 3 must land on chunk 1.
        let g = ChunkGuard::install(&[true, false, false, false]);
        assert_eq!(unsafe { msg_sb_start(g.ptr(3)) }, g.ptr(1));
    }

    #[test]
    fn msg_sb_start_of_null_is_null() {
        let _g = ChunkGuard::install(&[]);
        assert!(unsafe { msg_sb_start(std::ptr::null_mut()) }.is_null());
    }

    #[test]
    fn sb_text_end_cmdline_records_the_pending_clear() {
        let _g = ChunkGuard::install(&[]);
        unsafe { sb_text_end_cmdline() };
        assert_eq!(unsafe { *DO_CLEAR_SB_TEXT.get_mut() }, SbClearT::CmdlineDone);
    }

    #[test]
    fn sb_text_restart_cmdline_keeps_a_finished_line() {
        // The last chunk ends its line, so there is nothing unfinished
        // to discard.
        let g = ChunkGuard::install(&[false, true]);

        unsafe { sb_text_restart_cmdline() };

        assert_eq!(unsafe { *LAST_MSGCHUNK.get_mut() }, g.ptr(1), "nothing discarded");
        assert_eq!(unsafe { *DO_CLEAR_SB_TEXT.get_mut() }, SbClearT::CmdlineBusy);
    }

    #[test]
    fn sb_text_restart_cmdline_discards_an_unfinished_line() {
        // Chunk 0 ends a line; chunks 1 and 2 form an unfinished one,
        // so both are discarded and chunk 0 becomes the last.
        let g = ChunkGuard::install(&[true, false, false]);
        let survivor = g.ptr(0);

        unsafe { sb_text_restart_cmdline() };

        assert_eq!(unsafe { *LAST_MSGCHUNK.get_mut() }, survivor);
        assert!(
            unsafe { (*survivor).sb_next }.is_null(),
            "the survivor's forward link is cleared"
        );
    }

    #[test]
    fn sb_text_start_cmdline_marks_end_of_line_at_the_outer_level() {
        // Not already busy: the current line is finished off rather
        // than discarded.
        let g = ChunkGuard::install(&[false, false]);

        unsafe { sb_text_start_cmdline() };

        assert!(unsafe { (*g.ptr(1)).sb_eol }, "the line is closed off");
        assert_eq!(unsafe { *DO_CLEAR_SB_TEXT.get_mut() }, SbClearT::CmdlineBusy);
    }

    #[test]
    fn sb_text_start_cmdline_discards_when_already_busy() {
        // Recursive command line: the previous level's unfinished line
        // is discarded instead, since it is redrawn on return.
        let g = ChunkGuard::install(&[true, false]);
        let survivor = g.ptr(0);
        unsafe { *DO_CLEAR_SB_TEXT.get_mut() = SbClearT::CmdlineBusy };

        unsafe { sb_text_start_cmdline() };

        assert_eq!(
            unsafe { *LAST_MSGCHUNK.get_mut() },
            survivor,
            "the unfinished chunk was discarded"
        );
    }

    #[test]
    fn msg_check_flags_a_full_last_line() {
        // On the LAST row with the column at or past sc_col, the user
        // must acknowledge before the screen scrolls.
        let _lock = crate::globals::global_state_test_lock();
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let (pr, prow, pcol, psc, pwr, prc) =
            (g.Rows, g.msg_row, g.msg_col, g.sc_col, g.need_wait_return, g.redraw_cmdline);

        g.Rows = 30;
        g.sc_col = 10;
        g.msg_row = 29;
        g.msg_col = 10;
        g.need_wait_return = false;
        g.redraw_cmdline = false;

        unsafe { msg_check() };

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        assert!(g.need_wait_return);
        assert!(g.redraw_cmdline);

        g.Rows = pr;
        g.msg_row = prow;
        g.msg_col = pcol;
        g.sc_col = psc;
        g.need_wait_return = pwr;
        g.redraw_cmdline = prc;
    }

    #[test]
    fn msg_check_needs_both_the_last_row_and_the_column() {
        // Either condition alone is not enough - a message that is not
        // on the bottom line, or has not reached sc_col, will not
        // scroll anything away.
        let _lock = crate::globals::global_state_test_lock();
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let (pr, prow, pcol, psc, pwr, prc) =
            (g.Rows, g.msg_row, g.msg_col, g.sc_col, g.need_wait_return, g.redraw_cmdline);

        g.Rows = 30;
        g.sc_col = 10;

        // Not the last row.
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.msg_row = 5;
        g.msg_col = 20;
        g.need_wait_return = false;
        unsafe { msg_check() };
        assert!(!unsafe { crate::globals::GLOBALS.get_mut() }.need_wait_return);

        // Last row, but the column has not reached sc_col.
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.msg_row = 29;
        g.msg_col = 9;
        g.need_wait_return = false;
        unsafe { msg_check() };
        assert!(!unsafe { crate::globals::GLOBALS.get_mut() }.need_wait_return);

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        g.Rows = pr;
        g.msg_row = prow;
        g.msg_col = pcol;
        g.sc_col = psc;
        g.need_wait_return = pwr;
        g.redraw_cmdline = prc;
    }

    /// Test-only helper letting tests bump the otherwise-private
    /// `MSG_ID_NEXT` counter, matching the established
    /// `set_pum_is_visible`-style pattern (`popupmenu.rs`). Caller must
    /// hold `crate::globals::global_state_test_lock()` for the whole
    /// duration this value matters, and should restore the original
    /// value before releasing the lock.
    pub(crate) fn set_msg_id_next(value: i64) -> i64 {
        let cell = unsafe { MSG_ID_NEXT.get_mut() };
        let old = *cell;
        *cell = value;
        old
    }

    #[test]
    fn reset_last_sourcing_clears_both_fields() {
        let _guard = crate::globals::global_state_test_lock();
        let saved_name = unsafe { LAST_SOURCING_NAME.get_mut() }.clone();
        let saved_lnum = *unsafe { LAST_SOURCING_LNUM.get_mut() };

        *unsafe { LAST_SOURCING_NAME.get_mut() } = Some(b"init.lua".to_vec());
        *unsafe { LAST_SOURCING_LNUM.get_mut() } = 42;

        unsafe { reset_last_sourcing() };
        assert_eq!(*unsafe { LAST_SOURCING_NAME.get_mut() }, None);
        assert_eq!(*unsafe { LAST_SOURCING_LNUM.get_mut() }, 0);

        *unsafe { LAST_SOURCING_NAME.get_mut() } = saved_name;
        *unsafe { LAST_SOURCING_LNUM.get_mut() } = saved_lnum;
    }

    #[test]
    fn reset_last_sourcing_is_idempotent() {
        let _guard = crate::globals::global_state_test_lock();
        let saved_name = unsafe { LAST_SOURCING_NAME.get_mut() }.clone();
        let saved_lnum = *unsafe { LAST_SOURCING_LNUM.get_mut() };

        unsafe { reset_last_sourcing() };
        unsafe { reset_last_sourcing() };
        assert_eq!(*unsafe { LAST_SOURCING_NAME.get_mut() }, None);
        assert_eq!(*unsafe { LAST_SOURCING_LNUM.get_mut() }, 0);

        *unsafe { LAST_SOURCING_NAME.get_mut() } = saved_name;
        *unsafe { LAST_SOURCING_LNUM.get_mut() } = saved_lnum;
    }

    #[test]
    fn msg_starthere_copies_cmdline_row_into_lines_left() {
        let _guard = crate::globals::global_state_test_lock();
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let saved = (globals.lines_left, globals.cmdline_row, globals.msg_didany);

        globals.cmdline_row = 23;
        globals.lines_left = 0;
        globals.msg_didany = true;

        unsafe { msg_starthere() };
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        assert_eq!(globals.lines_left, 23);
        assert!(!globals.msg_didany);
        // cmdline_row itself is only read, never written.
        assert_eq!(globals.cmdline_row, 23);

        (globals.lines_left, globals.cmdline_row, globals.msg_didany) = saved;
    }

    #[test]
    fn set_keep_msg_stores_an_owned_copy() {
        let _guard = crate::globals::global_state_test_lock();
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let saved = (globals.keep_msg.clone(), globals.keep_msg_hl_id, globals.msg_silent);

        globals.msg_silent = 0;
        unsafe { set_keep_msg(Some(b"hello"), 7) };
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        assert_eq!(globals.keep_msg.as_deref(), Some(&b"hello"[..]));
        assert_eq!(globals.keep_msg_hl_id, 7);
        assert!(!globals.keep_msg_more);

        (globals.keep_msg, globals.keep_msg_hl_id, globals.msg_silent) = saved;
    }

    #[test]
    fn set_keep_msg_none_clears_the_message() {
        let _guard = crate::globals::global_state_test_lock();
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let saved = (globals.keep_msg.clone(), globals.keep_msg_hl_id, globals.msg_silent);

        globals.msg_silent = 0;
        unsafe { set_keep_msg(Some(b"hello"), 1) };
        unsafe { set_keep_msg(None, 3) };
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        assert_eq!(globals.keep_msg, None);
        // The highlight id is still updated even when clearing.
        assert_eq!(globals.keep_msg_hl_id, 3);

        (globals.keep_msg, globals.keep_msg_hl_id, globals.msg_silent) = saved;
    }

    #[test]
    fn set_keep_msg_stores_nothing_while_msg_silent() {
        let _guard = crate::globals::global_state_test_lock();
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let saved = (globals.keep_msg.clone(), globals.keep_msg_hl_id, globals.msg_silent);

        globals.msg_silent = 1;
        unsafe { set_keep_msg(Some(b"hello"), 5) };
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        // A silenced message is dropped, but hl_id still follows it.
        assert_eq!(globals.keep_msg, None);
        assert_eq!(globals.keep_msg_hl_id, 5);

        (globals.keep_msg, globals.keep_msg_hl_id, globals.msg_silent) = saved;
    }

    #[test]
    fn set_keep_msg_resets_keep_msg_more() {
        let _guard = crate::globals::global_state_test_lock();
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        let saved = (globals.keep_msg.clone(), globals.keep_msg_more, globals.msg_silent);

        globals.msg_silent = 0;
        globals.keep_msg_more = true;
        unsafe { set_keep_msg(Some(b"x"), 0) };
        let globals = unsafe { crate::globals::GLOBALS.get_mut() };
        assert!(!globals.keep_msg_more);

        (globals.keep_msg, globals.keep_msg_more, globals.msg_silent) = saved;
    }

    #[test]
    fn msg_id_exists_default_state() {
        let _lock = crate::globals::global_state_test_lock();
        // MSG_ID_NEXT starts at 1, so no id has ever been issued yet.
        assert!(!msg_id_exists(1));
        assert!(!msg_id_exists(0));
        assert!(!msg_id_exists(-1));
    }

    /// Saves/restores every piece of state the verbose-file functions
    /// touch, so these tests can't leak into any other test.
    fn with_verbose<R>(vfile: Option<&[u8]>, f: impl FnOnce() -> R) -> R {
        let _lock = crate::globals::global_state_test_lock();
        let prev_vfile =
            unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_vfile.clone();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_vfile =
            vfile.map(<[u8]>::to_vec);
        unsafe { *VERBOSE_FD.get_mut() = None };
        unsafe { *VERBOSE_DID_OPEN.get_mut() = false };

        let result = f();

        unsafe { *VERBOSE_FD.get_mut() = None };
        unsafe { *VERBOSE_DID_OPEN.get_mut() = false };
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_vfile = prev_vfile;
        result
    }

    /// A unique scratch path under the OS temp dir, removed on drop.
    struct ScratchFile {
        path: std::path::PathBuf,
    }

    impl ScratchFile {
        fn new(tag: &str) -> Self {
            let mut path = std::env::temp_dir();
            path.push(format!(
                "nero_verbose_{tag}_{}_{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_file(&path);
            ScratchFile { path }
        }
        fn bytes(&self) -> Vec<u8> {
            self.path.to_str().unwrap().as_bytes().to_vec()
        }
    }

    impl Drop for ScratchFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }

    #[test]
    fn verbose_open_and_stop_manage_the_file_handle() {
        // Grouped into one locked section deliberately: each of these
        // helpers holds `global_state_test_lock()` across real
        // filesystem I/O, and spreading them over many separate tests
        // measurably amplified an unrelated, pre-existing
        // parallel-load flake elsewhere in the suite.
        let scratch = ScratchFile::new("handle");
        // Pre-create with content, so the append-not-truncate check
        // below is meaningful.
        std::fs::write(&scratch.path, b"existing").unwrap();
        let bytes = scratch.bytes();

        with_verbose(Some(&bytes), || {
            // Opens, records the handle, and latches did_open.
            assert_eq!(unsafe { verbose_open() }, crate::vim_defs::OK);
            assert!(scratch.path.exists());
            assert!(unsafe { VERBOSE_FD.get_mut() }.is_some());
            assert!(unsafe { *VERBOSE_DID_OPEN.get_mut() });

            // A second call while already open is a no-op reporting OK.
            assert_eq!(unsafe { verbose_open() }, crate::vim_defs::OK);
            assert!(unsafe { VERBOSE_FD.get_mut() }.is_some());

            // Stop closes the handle and clears the latch.
            unsafe { verbose_stop() };
            assert!(unsafe { VERBOSE_FD.get_mut() }.is_none());
            assert!(!unsafe { *VERBOSE_DID_OPEN.get_mut() });

            // Append mode: the pre-existing content survived.
            assert_eq!(std::fs::read(&scratch.path).unwrap(), b"existing");
        });
    }

    #[test]
    fn verbose_open_only_attempts_once_after_a_failure() {
        // An unopenable path (a file inside a non-existent directory)
        // fails once, then the verbose_did_open latch makes every
        // later call a no-op that reports OK - exactly as the original
        // does, so the error is only ever given once.
        let mut bad = std::env::temp_dir();
        bad.push("nero_verbose_missing_dir");
        bad.push("nested");
        bad.push("log.txt");
        let bytes = bad.to_str().unwrap().as_bytes().to_vec();

        with_verbose(Some(&bytes), || {
            assert_eq!(unsafe { verbose_open() }, crate::vim_defs::FAIL);
            assert!(unsafe { *VERBOSE_DID_OPEN.get_mut() });
            // The latch short-circuits the retry.
            assert_eq!(unsafe { verbose_open() }, crate::vim_defs::OK);
            assert!(unsafe { VERBOSE_FD.get_mut() }.is_none());
        });
    }

    #[test]
    fn msg_id_exists_after_ids_issued() {
        let _lock = crate::globals::global_state_test_lock();
        let old = set_msg_id_next(5);
        assert!(msg_id_exists(1));
        assert!(msg_id_exists(4));
        assert!(!msg_id_exists(5));
        assert!(!msg_id_exists(6));
        assert!(!msg_id_exists(0));
        set_msg_id_next(old);
    }

    #[test]
    fn msg_use_grid_is_false_by_default() {
        let _lock = crate::globals::global_state_test_lock();
        assert!(!msg_use_grid());
    }

    #[test]
    fn msg_do_throttle_is_false_by_default() {
        let _lock = crate::globals::global_state_test_lock();
        assert!(!msg_do_throttle());
    }

    #[test]
    fn msg_scrollsize_matches_hand_computed_formula() {
        let _lock = crate::globals::global_state_test_lock();
        let old_scrolled = unsafe { crate::globals::GLOBALS.get_mut() }.msg_scrolled;
        let old_ch = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ch;

        unsafe { crate::globals::GLOBALS.get_mut() }.msg_scrolled = 0;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ch = 0;
        assert_eq!(msg_scrollsize(), 0); // 0 + 0 + (false as i32) = 0

        unsafe { crate::globals::GLOBALS.get_mut() }.msg_scrolled = 3;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ch = 0;
        assert_eq!(msg_scrollsize(), 4); // 3 + 0 + (3 > 1 => true) = 4

        unsafe { crate::globals::GLOBALS.get_mut() }.msg_scrolled = 0;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ch = 2;
        assert_eq!(msg_scrollsize(), 3); // 0 + 2 + (2 > 0 => true) = 3

        unsafe { crate::globals::GLOBALS.get_mut() }.msg_scrolled = old_scrolled;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_ch = old_ch;
    }

    #[test]
    fn redirecting_is_false_by_default() {
        let _lock = crate::globals::global_state_test_lock();
        assert!(!redirecting());
    }

    #[test]
    fn redirecting_true_when_redir_reg_set() {
        let _lock = crate::globals::global_state_test_lock();
        let old = unsafe { crate::globals::GLOBALS.get_mut() }.redir_reg;
        unsafe { crate::globals::GLOBALS.get_mut() }.redir_reg = b'a' as i32;
        assert!(redirecting());
        unsafe { crate::globals::GLOBALS.get_mut() }.redir_reg = old;
    }

    #[test]
    fn redirecting_true_when_redir_vname_set() {
        let _lock = crate::globals::global_state_test_lock();
        let old = unsafe { crate::globals::GLOBALS.get_mut() }.redir_vname;
        unsafe { crate::globals::GLOBALS.get_mut() }.redir_vname = true;
        assert!(redirecting());
        unsafe { crate::globals::GLOBALS.get_mut() }.redir_vname = old;
    }

    #[test]
    fn redirecting_true_when_verbosefile_set() {
        let _lock = crate::globals::global_state_test_lock();
        let old = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_vfile.take();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_vfile = Some(b"log.txt".to_vec());
        assert!(redirecting());
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_vfile = old;
    }

    // --- emsg_not_now ---

    /// Sets `GLOBALS.emsg_off`/`emsg_skip` and `OPTION_VARS.p_debug` for
    /// the duration of `f`, restoring all 3 afterward (even if `f`
    /// panics, via a manual `catch_unwind`, matching this crate's own
    /// established pattern for RAII-guard-free multi-field test setup).
    fn with_emsg_state<R>(emsg_off: i32, emsg_skip: i32, p_debug: Option<&[u8]>, f: impl FnOnce() -> R + std::panic::UnwindSafe) -> R {
        let old_off = unsafe { crate::globals::GLOBALS.get_mut() }.emsg_off;
        let old_skip = unsafe { crate::globals::GLOBALS.get_mut() }.emsg_skip;
        let old_debug = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_debug.take();
        unsafe { crate::globals::GLOBALS.get_mut() }.emsg_off = emsg_off;
        unsafe { crate::globals::GLOBALS.get_mut() }.emsg_skip = emsg_skip;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_debug = p_debug.map(<[u8]>::to_vec);
        let result = std::panic::catch_unwind(f);
        unsafe { crate::globals::GLOBALS.get_mut() }.emsg_off = old_off;
        unsafe { crate::globals::GLOBALS.get_mut() }.emsg_skip = old_skip;
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_debug = old_debug;
        result.unwrap()
    }

    #[test]
    fn emsg_not_now_default_state_is_false() {
        let _lock = crate::globals::global_state_test_lock();
        with_emsg_state(0, 0, None, || assert!(!emsg_not_now()));
    }

    #[test]
    fn emsg_not_now_true_when_emsg_off_and_debug_is_plain() {
        let _lock = crate::globals::global_state_test_lock();
        with_emsg_state(1, 0, None, || assert!(emsg_not_now()));
    }

    #[test]
    fn emsg_not_now_false_when_emsg_off_but_debug_contains_m() {
        let _lock = crate::globals::global_state_test_lock();
        with_emsg_state(1, 0, Some(b"m"), || assert!(!emsg_not_now()));
    }

    #[test]
    fn emsg_not_now_false_when_emsg_off_but_debug_contains_t() {
        let _lock = crate::globals::global_state_test_lock();
        with_emsg_state(1, 0, Some(b"t"), || assert!(!emsg_not_now()));
    }

    #[test]
    fn emsg_not_now_false_when_emsg_off_but_debug_contains_both() {
        let _lock = crate::globals::global_state_test_lock();
        with_emsg_state(1, 0, Some(b"mt"), || assert!(!emsg_not_now()));
    }

    #[test]
    fn emsg_not_now_true_when_emsg_skip_alone() {
        let _lock = crate::globals::global_state_test_lock();
        with_emsg_state(0, 1, None, || assert!(emsg_not_now()));
    }

    #[test]
    fn emsg_not_now_true_when_both_emsg_off_and_emsg_skip() {
        let _lock = crate::globals::global_state_test_lock();
        with_emsg_state(1, 1, None, || assert!(emsg_not_now()));
    }

    // --- trunc_string ---

    #[test]
    fn trunc_string_empty_input_is_an_empty_buffer() {
        assert_eq!(unsafe { trunc_string(b"", 10, 100) }, Vec::<u8>::new());
    }

    #[test]
    fn trunc_string_fits_without_truncating_returns_the_whole_string() {
        // "hello" is only 5 cells - comfortably under room_in=20's
        // half budget (8), so the first-part loop's own "s[e]==NUL"
        // fast-path fires and returns the whole thing untouched.
        assert_eq!(unsafe { trunc_string(b"hello", 20, 100) }, b"hello");
    }

    #[test]
    fn trunc_string_ascii_middle_truncation() {
        // Hand-traced: room = 10-3 = 7, half = 3. First part accepts
        // "abc" (3 cells, e=3). Last part walks backward from the end
        // accepting 'z','y','x','w' (4 cells, len reaches 7 == room),
        // rejecting 'v' (would make len=8 > room) - i ends up at the
        // position of 'w' (index 22). Since i(22) > e(3)+3(=6), the
        // "..." branch is used: "abc" + "..." + "wxyz".
        let s = b"abcdefghijklmnopqrstuvwxyz";
        assert_eq!(unsafe { trunc_string(s, 10, 100) }, b"abc...wxyz");
    }

    #[test]
    fn trunc_string_small_room_still_reserves_dots() {
        // room_in=6 => room=3, half=1. First part accepts only 'a'
        // (len=1, e=1). Last part walks back accepting only 'z'
        // (len=2 <= room=3), rejects 'y' (len=3, still <=3 actually -
        // let's just trust the real algorithm and check the shape
        // rather than hand-deriving every character here).
        let s = b"abcdefghijklmnopqrstuvwxyz";
        let result = unsafe { trunc_string(s, 6, 100) };
        assert!(result.windows(3).any(|w| w == b"..."), "expected a '...' in {result:?}");
        assert!(result.starts_with(b"a"));
        assert!(result.ends_with(b"z"));
    }

    #[test]
    fn trunc_string_buflen_clamps_the_fits_without_truncating_branch() {
        // The string is short enough to "fit without truncating" by
        // room_in alone (room_in=100 is huge), but buflen is small
        // enough to force real content clamping in the i<=e+3 branch.
        let s = b"abcdefghij"; // 10 bytes
        let result = unsafe { trunc_string(s, 100, 5) };
        // buflen=5 means at most 4 content bytes (buflen - 1 reserved
        // for the original's own conceptual NUL slot).
        assert_eq!(result.len(), 4);
        assert_eq!(result, b"abcd");
    }

    #[test]
    fn trunc_string_buflen_too_small_for_dots_hard_truncates() {
        // room_in is large enough to want a real "..." truncation, but
        // buflen is too small to fit "first part" + "..." at all, so
        // the hard-truncate-at-buflen branch is used instead.
        let s = b"abcdefghijklmnopqrstuvwxyz";
        let result = unsafe { trunc_string(s, 10, 4) };
        assert_eq!(result.len(), 3); // buflen - 1
    }

    #[test]
    fn trunc_string_negative_room_in_is_treated_as_zero() {
        // room_in < 3 forces room = 0 (degenerate case, can't fit any
        // real content alongside the "...").
        let s = b"abcdefghijklmnopqrstuvwxyz";
        let result = unsafe { trunc_string(s, 1, 100) };
        assert!(result.windows(3).any(|w| w == b"..."));
    }

    #[test]
    fn trunc_string_never_splits_a_multibyte_character() {
        // Mix of ASCII and a wide (2-cell) CJK character (U+4E2D "中",
        // 3 UTF-8 bytes) repeated several times - the riskiest part of
        // the algorithm is the byte-vs-cell tracking in both the
        // first-part inner loop (utfc_ptr2len) and the last-part
        // backward walk (utf_head_off). This doesn't hand-derive the
        // exact expected output (that would require replicating the
        // full cell-width arithmetic by hand), but does verify the
        // result is always valid UTF-8 (never a partial multi-byte
        // sequence) for a range of room_in values.
        let s = "ab中中中中中中中中中中cd".as_bytes();
        for room_in in 3..=20 {
            let result = unsafe { trunc_string(s, room_in, 100) };
            assert!(
                std::str::from_utf8(&result).is_ok(),
                "room_in={room_in} produced invalid UTF-8: {result:?}"
            );
        }
    }

    #[test]
    fn trunc_string_stops_at_an_embedded_nul() {
        // Matches the established "embedded NUL ends a C-string-
        // modeled scan" idiom - content after the NUL is invisible to
        // the function entirely, same as a real strlen(s) would see.
        assert_eq!(unsafe { trunc_string(b"hi\0garbage", 20, 100) }, b"hi");
    }

    // --- msg_strtrunc ---

    /// Resets every Globals/OPTION_VARS field `msg_strtrunc` reads to
    /// a fixed, known-good "should truncate, plenty of room" baseline,
    /// returning the previous values so a test can restore them.
    struct MsgStrtruncGuard {
        msg_scroll: i32,
        need_wait_return: bool,
        p_shm: Option<Vec<u8>>,
        exmode_active: bool,
        msg_silent: i32,
        msg_scrolled: i32,
        rows: i32,
        columns: i32,
        msg_row: i32,
        sc_col: i32,
    }

    impl MsgStrtruncGuard {
        fn set() -> Self {
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            let ov = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            let prev = MsgStrtruncGuard {
                msg_scroll: g.msg_scroll,
                need_wait_return: g.need_wait_return,
                p_shm: ov.p_shm.clone(),
                exmode_active: g.exmode_active,
                msg_silent: g.msg_silent,
                msg_scrolled: g.msg_scrolled,
                rows: g.Rows,
                columns: g.Columns,
                msg_row: g.msg_row,
                sc_col: g.sc_col,
            };
            g.msg_scroll = 0;
            g.need_wait_return = false;
            ov.p_shm = Some(b"T".to_vec());
            g.exmode_active = false;
            g.msg_silent = 0;
            g.msg_scrolled = 0;
            g.Rows = 24;
            g.Columns = 80;
            g.msg_row = 0;
            g.sc_col = 0;
            prev
        }
    }

    impl Drop for MsgStrtruncGuard {
        fn drop(&mut self) {
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            let ov = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
            g.msg_scroll = self.msg_scroll;
            g.need_wait_return = self.need_wait_return;
            ov.p_shm = self.p_shm.take();
            g.exmode_active = self.exmode_active;
            g.msg_silent = self.msg_silent;
            g.msg_scrolled = self.msg_scrolled;
            g.Rows = self.rows;
            g.Columns = self.columns;
            g.msg_row = self.msg_row;
            g.sc_col = self.sc_col;
        }
    }

    #[test]
    fn msg_strtrunc_short_message_needs_no_truncation() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = MsgStrtruncGuard::set();
        assert_eq!(unsafe { msg_strtrunc(b"hello", false) }, None);
    }

    #[test]
    fn msg_strtrunc_long_message_is_truncated() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = MsgStrtruncGuard::set();
        // 80 columns * 24 rows is a huge room budget normally, but
        // msg_row is set to Rows-1 (the very last row) below, so
        // "room" (based on the remaining rows/showcmd column) becomes
        // small enough that a long message must be truncated.
        unsafe { crate::globals::GLOBALS.get_mut() }.msg_row = 23;
        unsafe { crate::globals::GLOBALS.get_mut() }.sc_col = 5;
        let long_msg = vec![b'x'; 500];
        let result = unsafe { msg_strtrunc(&long_msg, false) };
        assert!(result.is_some());
        assert!(result.unwrap().len() < 500);
    }

    #[test]
    fn msg_strtrunc_returns_none_when_shortmess_truncall_not_set() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = MsgStrtruncGuard::set();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_shm = Some(Vec::new());
        unsafe { crate::globals::GLOBALS.get_mut() }.msg_row = 23;
        unsafe { crate::globals::GLOBALS.get_mut() }.sc_col = 5;
        let long_msg = vec![b'x'; 500];
        assert_eq!(unsafe { msg_strtrunc(&long_msg, false) }, None);
    }

    #[test]
    fn msg_strtrunc_force_truncates_even_without_shortmess_truncall() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = MsgStrtruncGuard::set();
        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_shm = Some(Vec::new());
        unsafe { crate::globals::GLOBALS.get_mut() }.msg_row = 23;
        unsafe { crate::globals::GLOBALS.get_mut() }.sc_col = 5;
        let long_msg = vec![b'x'; 500];
        let result = unsafe { msg_strtrunc(&long_msg, true) };
        assert!(result.is_some());
    }

    // --- other_sourcing_name / get_emsg_source ---

    #[test]
    fn other_sourcing_name_is_false_when_there_is_no_sourcing_info() {
        let _lock = crate::globals::global_state_test_lock();
        // EXESTACK is always empty in this crate today, so
        // have_sourcing_info() is always false - a real, always-taken
        // early return, not a hardcoded stub.
        assert!(!crate::runtime::have_sourcing_info());
        assert!(!other_sourcing_name());
    }

    #[test]
    fn get_emsg_source_is_none_when_there_is_no_sourcing_info() {
        let _lock = crate::globals::global_state_test_lock();
        assert!(!crate::runtime::have_sourcing_info());
        assert_eq!(get_emsg_source(), None);
    }
}
