//! Translated from `src/nvim/buffer.c` (tractable core only). This is one
//! of the largest, most cross-cutting files in the whole codebase
//! (~4349 lines, 91 top-level functions): almost every function needs
//! real file I/O (`readfile`/`ml_open`, `os/fs.c`), `ctx_switch`/window
//! management, `autocmd.c`'s `apply_autocmds`, the eval engine's `b:`
//! dict watcher machinery, or `memline.c`'s `ml_get`/`ml_get_buf`.
//!
//! Translated: `calc_percentage`, `get_highest_fnum` (+ its own
//! `top_file_num` private counter, mirrored as `TOP_FILE_NUM` since
//! the original keeps it as a file-static, not an `EXTERN` global -
//! same treatment as `memfile.c`'s own per-file statics), `set_bufref`/
//! `bufref_valid`/`buf_valid` (+ its own `buf_free_count` private
//! counter, `BUF_FREE_COUNT`), the `'buftype'`-testing predicate
//! family `bt_prompt`/`bt_cmdwin`/`bt_help`/`bt_normal`/`bt_quickfix`/
//! `bt_terminal`/`bt_nofilename`/`bt_nofile`/`bt_dontwrite`, `buf_hide`,
//! `buf_is_empty` (now tractable now that `memline.c`'s `ml_get_buf`
//! exists), `buffer.h`'s `buf_meta_total` (a tiny `static inline`
//! header function, not from `buffer.c` itself - harvested for
//! `drawscreen.c`'s `number_width`); and now that `eval/typval_defs.rs`'s
//! `TypvalT`/`ChangedtickDictItem` are real (not opaque placeholders),
//! `buf_get_changedtick`/`buf_set_changedtick`/`buf_inc_changedtick`/
//! `buf_init_changedtick` - each skips only the real dict-watcher
//! notification/`b_vars` registration side effect specifically (see
//! `buf_set_changedtick`/`buf_init_changedtick`'s own doc comments for
//! exactly what each still needs - `DictT`'s own `watchers` field and
//! a sound `ChangedtickDictItem`-as-`dictitem_T` lookup mechanism,
//! respectively, neither of which is simply "`dict_T` doesn't exist"
//! anymore), keeping the underlying `b:changedtick` value itself fully
//! correct for every other C-level accessor in this crate. `set_buflisted`
//! (now tractable now that `autocmd.c`'s `apply_autocmds` is real - see
//! `crate::autocmd`'s own module doc).
//!
//! Deferred (each needs a not-yet-translated subsystem):
//! - `bt_nofileread` (`static`): its only caller, `open_buffer`, is
//!   itself deferred (real file I/O) - translating it now would be
//!   genuinely dead code.
//! - `bt_dontwrite_msg`: needs `emsg()` (`message.c`).
//! - `read_buffer`/`buf_ensure_loaded`/`open_buffer`/`close_buffer`/
//!   `buf_freeall`/`free_buffer`/`buflist_new`/`buflist_getfile`/
//!   `buflist_findnr`/etc.: need real file I/O, `ctx_switch` (window
//!   management), and `autocmd.c`.
//! - `buf_contents_changed`/`wipe_buffer`: need `buflist_new`,
//!   `ctx_switch` (the real switch, not just `ctx_restore`'s bypass
//!   path), `block_autocmds`/`unblock_autocmds`, `readfile`, and
//!   `close_buffer` - `apply_autocmds` alone isn't enough to unblock
//!   these two (re-verified directly against the real source, not
//!   assumed from the old blanket note grouping them with
//!   `set_buflisted`).
//! - Everything else in this file (buffer-list management, window-
//!   buffer association, `:buffer`/`:ls`/title-bar formatting, modeline
//!   processing, etc.): each needs 2+ of the above, plus `tag.c`/
//!   `quickfix.c`/`window.c`.

use crate::buffer_defs::{BufT, BufrefT};
use crate::ex_cmds_defs::cmod;
use crate::globals::{GlobalCell, GLOBALS};
use crate::option_vars::OPTION_VARS;

/// Highest-ever-assigned buffer number counter (`buffer.c`'s own
/// file-static `top_file_num`), starting at 1 like the original.
static TOP_FILE_NUM: GlobalCell<i32> = GlobalCell::new(1);

/// Incremented every time a `buf_T` is freed, letting [`bufref_valid`]
/// skip a full buffer-list walk when nothing has been freed since
/// [`set_bufref`] was called (`buffer.c`'s own file-static
/// `buf_free_count`).
static BUF_FREE_COUNT: GlobalCell<i32> = GlobalCell::new(0);

/// Returns byte `idx` of an option modeled as `Option<Vec<u8>>`, or NUL
/// (`0`) if unset/short - matches how the original dereferences
/// `buf->b_p_bt[idx]`/`buf->b_p_bh[idx]` (a `char *` that in practice is
/// always at least NUL-terminated, never truly `NULL`).
fn opt_byte(opt: &Option<Vec<u8>>, idx: usize) -> u8 {
    opt.as_deref().and_then(|s| s.get(idx)).copied().unwrap_or(0)
}

/// Calculate the percentage that `part` is of `whole` (`calc_percentage`).
#[must_use]
pub fn calc_percentage(part: i64, whole: i64) -> i32 {
    // With 32 bit longs and more than 21,474,836 lines multiplying by 100
    // causes an overflow, thus for large numbers divide instead.
    if part > 1_000_000 {
        (part / (whole / 100)) as i32
    } else {
        ((part * 100) / whole) as i32
    }
}

/// The highest possible buffer number (`get_highest_fnum`).
///
/// # Safety
/// Touches a `GlobalCell` - same requirement as every other function
/// that does so: no overlapping live access.
#[must_use]
pub unsafe fn get_highest_fnum() -> i32 {
    unsafe { *TOP_FILE_NUM.get_mut() - 1 }
}

/// Fill in `bufref` to later check with [`bufref_valid`] whether `buf` is
/// still a valid, live buffer (`set_bufref`).
///
/// # Safety
/// Touches a `GlobalCell` - same requirement as every other function
/// that does so: no overlapping live access.
pub unsafe fn set_bufref(bufref: &mut BufrefT, buf: Option<&BufT>) {
    bufref.br_buf = match buf {
        Some(b) => b as *const BufT as *mut BufT,
        None => std::ptr::null_mut(),
    };
    bufref.br_fnum = buf.map_or(0, |b| b.handle);
    bufref.br_buf_free_count = unsafe { *BUF_FREE_COUNT.get_mut() };
}

/// Return true if `bufref->br_buf` points to the same buffer as when
/// [`set_bufref`] was called and it is a valid buffer. Only goes through
/// the buffer list if `buf_free_count` changed. Also checks if `b_fnum`
/// is still the same, since a `:bwipe` followed by `:new` might get the
/// same allocated memory, but it's a different buffer (`bufref_valid`).
///
/// # Safety
/// `bufref.br_buf`, if non-null, must point to a live `BufT` (or one
/// still reachable via the `lastbuf`/`b_prev` chain). Touches a
/// `GlobalCell` - same requirement as every other function that does so:
/// no overlapping live access.
#[must_use]
pub unsafe fn bufref_valid(bufref: &BufrefT) -> bool {
    if bufref.br_buf_free_count == unsafe { *BUF_FREE_COUNT.get_mut() } {
        return true;
    }
    (unsafe { buf_valid(bufref.br_buf) })
        && bufref.br_fnum == unsafe { &*bufref.br_buf }.handle
}

/// Check that `buf` points to a valid buffer in the buffer list. Can be
/// slow if there are many buffers, prefer using [`bufref_valid`]
/// (`buf_valid`).
///
/// # Safety
/// Touches `crate::globals::GLOBALS` - same requirement as every other
/// function that does so: no overlapping live access.
#[must_use]
pub unsafe fn buf_valid(buf: *const BufT) -> bool {
    if buf.is_null() {
        return false;
    }
    // Assume that we more often have a recent buffer, start with the
    // last one.
    let mut bp: *mut BufT = unsafe { GLOBALS.get_mut() }.lastbuf;
    while !bp.is_null() {
        if std::ptr::eq(bp as *const BufT, buf) {
            return true;
        }
        bp = unsafe { &*bp }.b_prev;
    }
    false
}

/// `true` if `buf` is a prompt buffer (`bt_prompt`).
#[must_use]
pub fn bt_prompt(buf: Option<&BufT>) -> bool {
    buf.is_some_and(|b| opt_byte(&b.b_p_bt, 0) == b'p')
}

/// `true` if `buf` is the `cmdwin` scratch buffer (`bt_cmdwin`).
///
/// # Safety
/// Touches `crate::globals::GLOBALS` - same requirement as every other
/// function that does so: no overlapping live access.
#[must_use]
pub unsafe fn bt_cmdwin(buf: Option<&BufT>) -> bool {
    match buf {
        Some(b) => std::ptr::eq(
            b as *const BufT,
            unsafe { GLOBALS.get_mut() }.cmdwin_buf as *const BufT,
        ),
        None => false,
    }
}

/// `true` if `buf` is a help buffer (`bt_help`).
#[must_use]
pub fn bt_help(buf: Option<&BufT>) -> bool {
    buf.is_some_and(|b| b.b_help)
}

/// `true` if `buf` has `'buftype'` empty, i.e. a normal buffer
/// (`bt_normal`).
#[must_use]
pub fn bt_normal(buf: Option<&BufT>) -> bool {
    buf.is_some_and(|b| opt_byte(&b.b_p_bt, 0) == 0)
}

/// `true` if `buf` is a quickfix buffer (`bt_quickfix`).
#[must_use]
pub fn bt_quickfix(buf: Option<&BufT>) -> bool {
    buf.is_some_and(|b| opt_byte(&b.b_p_bt, 0) == b'q')
}

/// `true` if `buf` is a terminal buffer (`bt_terminal`).
#[must_use]
pub fn bt_terminal(buf: Option<&BufT>) -> bool {
    buf.is_some_and(|b| opt_byte(&b.b_p_bt, 0) == b't')
}

/// `true` if `buf` is "nofile", "acwrite", a terminal, or a prompt
/// buffer - i.e. has no real backing file name (`bt_nofilename`).
#[must_use]
pub fn bt_nofilename(buf: Option<&BufT>) -> bool {
    buf.is_some_and(|b| {
        (opt_byte(&b.b_p_bt, 0) == b'n' && opt_byte(&b.b_p_bt, 2) == b'f')
            || opt_byte(&b.b_p_bt, 0) == b'a'
            || !b.terminal.is_null()
            || opt_byte(&b.b_p_bt, 0) == b'p'
    })
}

/// `true` if `buf` has `'buftype'` set to "nofile" (`bt_nofile`).
#[must_use]
pub fn bt_nofile(buf: Option<&BufT>) -> bool {
    buf.is_some_and(|b| opt_byte(&b.b_p_bt, 0) == b'n' && opt_byte(&b.b_p_bt, 2) == b'f')
}

/// `true` if `buf` is "nowrite", "nofile", terminal, or prompt - i.e.
/// should not be written to its file (`bt_dontwrite`).
#[must_use]
pub fn bt_dontwrite(buf: Option<&BufT>) -> bool {
    buf.is_some_and(|b| {
        opt_byte(&b.b_p_bt, 0) == b'n' || !b.terminal.is_null() || opt_byte(&b.b_p_bt, 0) == b'p'
    })
}

/// `true` if the buffer should be hidden, according to `'bufhidden'`,
/// `'hidden'`, and `":hide"` (`buf_hide`).
///
/// # Safety
/// Touches `crate::globals::GLOBALS` and `crate::option_vars::OPTION_VARS`,
/// each with the same requirement as every other function that touches
/// them: no overlapping live access.
#[must_use]
pub unsafe fn buf_hide(buf: &BufT) -> bool {
    // 'bufhidden' overrules 'hidden' and ":hide", check it first.
    match opt_byte(&buf.b_p_bh, 0) {
        b'u' | b'w' | b'd' => return false, // "unload"/"wipe"/"delete"
        b'h' => return true,                // "hide"
        _ => {}
    }
    unsafe { OPTION_VARS.get_mut() }.p_hid != 0
        || unsafe { GLOBALS.get_mut() }.cmdmod.cmod_flags & cmod::HIDE != 0
}

/// Return true if `buf` is empty: exactly one line, and that line is
/// itself empty (`buf_is_empty`).
///
/// # Safety
/// `buf.b_ml.ml_mfp`, if non-null, must be a valid pointer to a live
/// `MemfileT` (same requirement as `crate::memline::ml_get_buf`).
#[must_use]
pub unsafe fn buf_is_empty(buf: &mut BufT) -> bool {
    buf.b_ml.ml_line_count == 1 && unsafe { crate::memline::ml_get_buf(buf, 1) }[0] == 0
}

/// Return the total count of a given kind of extmark metadata in
/// `buf` (`buf_meta_total`). Actually a `static inline` function in
/// `buffer.h`, not `buffer.c` itself - harvested here alongside its
/// real caller, `drawscreen.c`'s `number_width`, since `buffer.h` has
/// no dedicated module of its own in this crate.
#[must_use]
pub fn buf_meta_total(buf: &BufT, m: crate::marktree_defs::MetaIndex) -> u32 {
    buf.b_marktree.meta_root[m as usize]
}

/// Get `b:changedtick` value. Faster than querying `b:`
/// (`buf_get_changedtick`, `buffer.h`'s own `static inline`).
#[must_use]
pub fn buf_get_changedtick(buf: &BufT) -> crate::eval::typval_defs::VarnumberT {
    match buf.changedtick_di.di_tv.value {
        crate::eval::typval_defs::TypvalValue::Number(n) => n,
        // Not yet initialized via buf_init_changedtick - matches
        // reading an all-zero union in the original, which would also
        // read 0.
        _ => 0,
    }
}

/// Set `b:changedtick`, also checking `b:` for consistency in debug
/// builds in the original (`buf_set_changedtick`).
///
/// # Deferred
/// The original also notifies any dict watchers on `buf->b_vars` of
/// the change (`tv_dict_watcher_notify`) - not done here. `dict_T`
/// itself is real now (as [`crate::eval::typval_defs::DictT`]), but
/// its `watchers` field (a `QUEUE` of dict-key watchers set by user
/// code, e.g. `dictwatcheradd()`) is still deferred - needs a `QUEUE`
/// intrusive-linked-list translation first, see `DictT`'s own doc
/// comment. `b_vars` itself is also always null in this crate so far
/// (nothing allocates a real per-buffer dict yet - see
/// [`buf_init_changedtick`]'s own doc comment for the further
/// complication even once it is). The underlying value itself is
/// still set correctly, and every other C-level accessor in this
/// crate reads it directly (not through the dict), so this gap only
/// affects Vimscript-visible `b:changedtick` watchers, not this
/// crate's own internal bookkeeping.
pub fn buf_set_changedtick(buf: &mut BufT, changedtick: crate::eval::typval_defs::VarnumberT) {
    buf.changedtick_di.di_tv.value = crate::eval::typval_defs::TypvalValue::Number(changedtick);
}

/// Increment `b:changedtick` value. Also checks `b:` for consistency
/// in debug builds in the original (`buf_inc_changedtick`).
pub fn buf_inc_changedtick(buf: &mut BufT) {
    buf_set_changedtick(buf, buf_get_changedtick(buf) + 1);
}

/// Initialize `buf.changedtick_di` (`buf_init_changedtick`,
/// `static inline` in the original).
///
/// # Deferred
/// The original also registers this item into `buf->b_vars` (via
/// `tv_dict_add`) so Vimscript code can read `b:changedtick` through
/// the dict lookup path - not done here. This needs more than just
/// `dict_T` (real now, and `b_vars` is `*mut DictT` already - see
/// `buffer_defs.rs`): the original casts `&buf->changedtick_di` (a
/// `ChangedtickDictItem`, the fixed-key-size `TV_DICTITEM_STRUCT`
/// instantiation) to a plain `dictitem_T *`, relying on their
/// byte-identical C struct layout - a cast this crate's separate,
/// unrelated `ChangedtickDictItem`/`DictitemT` Rust types cannot
/// soundly replicate (and [`crate::eval::typval_defs::DictT`]'s own
/// `dv_index` side table is typed `*mut DictitemT` specifically, not
/// an untyped pointer, so it has nowhere to put a
/// `*mut ChangedtickDictItem` even if the cast were sound). A real
/// fix needs its own design pass (e.g. a shared trait, an untyped
/// `dv_index` value, or a different lookup mechanism entirely) -
/// deliberately not attempted here. `b_vars` is also always null in
/// this crate so far, so there is nothing to insert into yet either.
/// [`buf_get_changedtick`]/[`buf_set_changedtick`] (this crate's own
/// C-level accessors) already read/write the real value directly,
/// independent of this dict registration.
pub fn buf_init_changedtick(buf: &mut BufT) {
    buf.changedtick_di = crate::eval::typval_defs::ChangedtickDictItem {
        di_flags: crate::eval::typval_defs::dict_item_flags::RO
            | crate::eval::typval_defs::dict_item_flags::FIX,
        di_tv: crate::eval::typval_defs::TypvalT {
            v_lock: crate::eval::typval_defs::VarLockStatus::Fixed,
            value: crate::eval::typval_defs::TypvalValue::Number(buf_get_changedtick(buf)),
        },
        di_key: b"changedtick".to_vec(),
    };
}

/// Set `'buflisted'` for `curbuf` to `on` and trigger autocommands if
/// it changed (`set_buflisted`).
///
/// Now tractable now that `autocmd.c`'s `apply_autocmds` is real (see
/// `crate::autocmd`'s own module doc) - currently always a real no-op
/// beyond the `b_p_bl` flip itself, since `AUTOCMDS` is always empty
/// today.
///
/// # Safety
/// `crate::globals::GLOBALS.curbuf` must be a valid, non-null pointer
/// to a live `BufT`.
pub unsafe fn set_buflisted(on: bool) {
    // SAFETY: forwarded from this function's own safety doc.
    let curbuf = unsafe { &mut *crate::globals::GLOBALS.get_mut().curbuf };
    let on = i32::from(on);
    if on == curbuf.b_p_bl {
        return;
    }

    curbuf.b_p_bl = on;
    let event = if on != 0 {
        crate::autocmd_defs::EventT::BufAdd
    } else {
        crate::autocmd_defs::EventT::BufDelete
    };
    let _ = crate::autocmd::apply_autocmds(event, None, None, false, Some(&*curbuf));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer_defs::BufT;

    #[test]
    fn buf_get_changedtick_defaults_to_zero_before_init() {
        let buf = BufT::default();
        assert_eq!(buf_get_changedtick(&buf), 0);
    }

    #[test]
    fn buf_set_and_get_changedtick_roundtrip() {
        let mut buf = BufT::default();
        buf_set_changedtick(&mut buf, 5);
        assert_eq!(buf_get_changedtick(&buf), 5);
    }

    #[test]
    fn buf_inc_changedtick_increments_by_one() {
        let mut buf = BufT::default();
        buf_set_changedtick(&mut buf, 5);
        buf_inc_changedtick(&mut buf);
        assert_eq!(buf_get_changedtick(&buf), 6);
    }

    #[test]
    fn buf_inc_changedtick_from_default_starts_at_one() {
        let mut buf = BufT::default();
        buf_inc_changedtick(&mut buf);
        assert_eq!(buf_get_changedtick(&buf), 1);
    }

    #[test]
    fn buf_init_changedtick_sets_flags_lock_and_key() {
        let mut buf = BufT::default();
        buf_init_changedtick(&mut buf);

        assert_eq!(
            buf.changedtick_di.di_flags,
            crate::eval::typval_defs::dict_item_flags::RO
                | crate::eval::typval_defs::dict_item_flags::FIX
        );
        assert_eq!(buf.changedtick_di.di_tv.v_lock, crate::eval::typval_defs::VarLockStatus::Fixed);
        assert_eq!(buf.changedtick_di.di_key, b"changedtick");
        // Starts at whatever buf_get_changedtick already reported (0
        // for a fresh, never-set buffer).
        assert_eq!(buf_get_changedtick(&buf), 0);
    }

    #[test]
    fn buf_init_changedtick_preserves_a_prior_value() {
        let mut buf = BufT::default();
        buf_set_changedtick(&mut buf, 42);
        buf_init_changedtick(&mut buf);
        assert_eq!(buf_get_changedtick(&buf), 42);
    }

    fn buf_with_bt(bt: &str) -> BufT {
        BufT {
            b_p_bt: Some(bt.as_bytes().to_vec()),
            ..Default::default()
        }
    }

    #[test]
    fn calc_percentage_matches_c_arithmetic() {
        assert_eq!(calc_percentage(50, 100), 50);
        assert_eq!(calc_percentage(1, 3), 33);
        assert_eq!(calc_percentage(2_000_000, 4_000_000), 50);
    }

    #[test]
    fn bt_prompt_checks_first_byte() {
        assert!(bt_prompt(Some(&buf_with_bt("prompt"))));
        assert!(!bt_prompt(Some(&buf_with_bt("nofile"))));
        assert!(!bt_prompt(None));
    }

    #[test]
    fn bt_normal_is_true_only_for_empty_buftype() {
        assert!(bt_normal(Some(&buf_with_bt(""))));
        assert!(!bt_normal(Some(&buf_with_bt("help"))));
    }

    #[test]
    fn bt_quickfix_and_terminal_check_first_byte() {
        assert!(bt_quickfix(Some(&buf_with_bt("quickfix"))));
        assert!(bt_terminal(Some(&buf_with_bt("terminal"))));
        assert!(!bt_quickfix(Some(&buf_with_bt("terminal"))));
    }

    #[test]
    fn bt_nofile_checks_no_and_f_bytes() {
        assert!(bt_nofile(Some(&buf_with_bt("nofile"))));
        assert!(!bt_nofile(Some(&buf_with_bt("nowrite"))));
    }

    #[test]
    fn bt_nofilename_covers_nofile_acwrite_terminal_prompt() {
        assert!(bt_nofilename(Some(&buf_with_bt("nofile"))));
        assert!(bt_nofilename(Some(&buf_with_bt("acwrite"))));
        assert!(bt_nofilename(Some(&buf_with_bt("prompt"))));
        assert!(!bt_nofilename(Some(&buf_with_bt("help"))));
    }

    #[test]
    fn bt_dontwrite_covers_nowrite_terminal_prompt() {
        assert!(bt_dontwrite(Some(&buf_with_bt("nowrite"))));
        assert!(bt_dontwrite(Some(&buf_with_bt("prompt"))));
        assert!(!bt_dontwrite(Some(&buf_with_bt("help"))));
    }

    #[test]
    fn bt_help_checks_b_help_flag() {
        let mut b = BufT::default();
        assert!(!bt_help(Some(&b)));
        b.b_help = true;
        assert!(bt_help(Some(&b)));
    }

    #[test]
    fn buf_valid_returns_false_for_null() {
        // SAFETY: no overlapping GLOBALS access from other threads in tests.
        unsafe {
            assert!(!buf_valid(std::ptr::null()));
        }
    }

    #[test]
    fn set_bufref_and_bufref_valid_roundtrip() {
        let buf = buf_with_bt("");
        let mut bufref = BufrefT::default();
        // SAFETY: single-threaded test, no overlapping GLOBALS access.
        unsafe {
            set_bufref(&mut bufref, Some(&buf));
            assert_eq!(bufref.br_fnum, buf.handle);
            // buf_free_count hasn't changed since set_bufref, so
            // bufref_valid takes the fast path (true) without needing
            // `buf` to actually be linked into the real buffer list.
            assert!(bufref_valid(&bufref));
        }
    }

    #[test]
    fn set_bufref_none_gives_null_buf_and_zero_fnum() {
        let mut bufref = BufrefT::default();
        // SAFETY: single-threaded test, no overlapping GLOBALS access.
        unsafe {
            set_bufref(&mut bufref, None);
        }
        assert!(bufref.br_buf.is_null());
        assert_eq!(bufref.br_fnum, 0);
    }

    #[test]
    fn buf_hide_bufhidden_overrules_hidden_and_cmdmod() {
        let mut b = buf_with_bt("");
        b.b_p_bh = Some(b"hide".to_vec());
        // SAFETY: single-threaded test, no overlapping GLOBALS/OPTION_VARS access.
        unsafe {
            assert!(buf_hide(&b));
            b.b_p_bh = Some(b"unload".to_vec());
            assert!(!buf_hide(&b));
        }
    }

    #[test]
    fn buf_is_empty_true_for_freshly_opened_buffer() {
        // ml_open touches shared GLOBALS.got_int internally via
        // mf_sync - must hold the lock like every other GlobalCell-
        // touching test.
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);

        assert!(unsafe { buf_is_empty(&mut buf) });

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn buf_is_empty_false_when_line_has_content() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(&mut buf, 1, b"x\0") },
            crate::vim_defs::OK
        );

        assert!(!unsafe { buf_is_empty(&mut buf) });

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    #[test]
    fn buf_is_empty_false_when_more_than_one_line() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        assert_eq!(
            unsafe { crate::memline::ml_append_buf(&mut buf, 1, b"\0", 1, false) },
            crate::vim_defs::OK
        );

        // Two empty lines: still not "empty" per buf_is_empty's own
        // definition (exactly one line).
        assert!(!unsafe { buf_is_empty(&mut buf) });

        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    /// Points `GLOBALS.curbuf` at `buf` for the guard's lifetime,
    /// restoring the previous value on drop. Callers must hold
    /// `global_state_test_lock()` for the guard's whole lifetime
    /// (matching `change.rs`'s own identically-named helper).
    struct CurbufGuard {
        previous: *mut BufT,
    }

    impl CurbufGuard {
        fn set(new_curbuf: *mut BufT) -> Self {
            let previous = unsafe { crate::globals::GLOBALS.get_mut() }.curbuf;
            unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = new_curbuf;
            CurbufGuard { previous }
        }
    }

    impl Drop for CurbufGuard {
        fn drop(&mut self) {
            unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = self.previous;
        }
    }

    #[test]
    fn set_buflisted_is_a_noop_when_value_unchanged() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { b_p_bl: 1, ..Default::default() };
        let _guard = CurbufGuard::set(&mut buf as *mut BufT);

        unsafe { set_buflisted(true) };

        assert_eq!(buf.b_p_bl, 1);
    }

    #[test]
    fn set_buflisted_flips_off_to_on() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { b_p_bl: 0, ..Default::default() };
        let _guard = CurbufGuard::set(&mut buf as *mut BufT);

        unsafe { set_buflisted(true) };

        assert_eq!(buf.b_p_bl, 1);
    }

    #[test]
    fn set_buflisted_flips_on_to_off() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = BufT { b_p_bl: 1, ..Default::default() };
        let _guard = CurbufGuard::set(&mut buf as *mut BufT);

        unsafe { set_buflisted(false) };

        assert_eq!(buf.b_p_bl, 0);
    }
}
