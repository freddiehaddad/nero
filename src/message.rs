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
//! `DEFAULT_GRID` is harvested here ahead of its real owning file,
//! `grid.c` (not translated) - it is the original's own file-static
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

/// `default_grid` - the main screen's own [`crate::grid_defs::ScreenGrid`],
/// harvested here from `grid.c` ahead of the rest of that file (see this
/// module's own doc comment). Stays at [`crate::grid_defs::ScreenGrid::default`]
/// (`SCREEN_GRID_INIT`, `chars` null) forever today, since nothing in
/// this crate can currently allocate a real grid.
static DEFAULT_GRID: LazyLock<GlobalCell<crate::grid_defs::ScreenGrid>> =
    LazyLock::new(|| GlobalCell::new(crate::grid_defs::ScreenGrid::default()));

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
    let has_chars = !unsafe { DEFAULT_GRID.get_mut() }.chars.is_null();
    has_chars && !crate::ui::ui_has(crate::ui::UiExtension::Messages)
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

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

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
    fn msg_id_exists_default_state() {
        let _lock = crate::globals::global_state_test_lock();
        // MSG_ID_NEXT starts at 1, so no id has ever been issued yet.
        assert!(!msg_id_exists(1));
        assert!(!msg_id_exists(0));
        assert!(!msg_id_exists(-1));
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
