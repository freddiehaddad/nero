//! Translated from `src/nvim/quickfix.c` (tractable core only).
//!
//! `quickfix.c` (~7000 lines) implements the `:make`/`:grep`/`:cfile`/
//! `:copen` quickfix-and-location-list subsystem: parsing compiler/grep
//! output into a `qf_list_T` of `qfline_T` entries, the quickfix window,
//! error navigation (`:cnext`/`:cprev`/etc.), and much more. Almost every
//! function needs `qf_info_T`'s real fields (still just an opaque
//! placeholder, [`crate::types_defs::QfInfoT`]), `errorformat` parsing,
//! or the quickfix window/buffer UI - none of which exist yet.
//!
//! Translated: [`qf_mark_adjust`] - `mark_adjust_buf`'s (`mark.c`) own
//! real dependency, needed to translate that function faithfully.
//! `qf_mark_adjust`'s OWN first, real check
//! (`buf->b_has_qf_entry & buf_has_flag`) is always false in this crate
//! today: nothing can currently populate `BufT.b_has_qf_entry` with
//! either flag (that only happens when a real quickfix/location list
//! actually gains an entry pointing at the buffer, via
//! `qf_fill_buffer`/`qf_alloc_entry`-style internals, none translated) -
//! so this function's entire remaining body (the real per-list,
//! per-entry line-number-adjustment walk) is genuinely, provably
//! unreachable today, and is not translated at all (not even as an
//! `unimplemented!()` stub) - matching this crate's established
//! "translate only the real, reachable fast path" precedent (e.g.
//! `win_lines_concealed`/`decor_conceal_line`/the whole `AUTOCMDS`-
//! empty-registry family). Also [`qf_fmt_text`] - a pure string
//! transform (collapse a newline plus any immediately-following
//! whitespace/newlines into a single space) that only touches a
//! caller-provided [`crate::garray_defs::GarrayT`] output buffer and a
//! plain `&[u8]` input, with no dependency on `qf_info_T`/`qf_list_T`
//! at all (unlike most of this file).
//!
//! The quickfix list STORAGE is now translated: `qfline_T`
//! ([`QflineT`]), `qfltype_T` ([`QfltypeT`]), `qf_list_T`
//! ([`QfListT`]) and `qf_info_T` ([`crate::types_defs::QfInfoT`], no
//! longer an opaque placeholder). That unblocks the small helpers
//! built directly on them: [`qf_stack_empty`], [`qf_list_empty`],
//! [`qf_list_has_valid_entries`], [`qf_get_list`], [`qf_get_curlist`]
//! and the `IS_QF_STACK`/`IS_LL_STACK`/`IS_QF_LIST`/`IS_LL_LIST`
//! macros ([`is_qf_stack`]/[`is_ll_stack`]/[`is_qf_list`]/
//! [`is_ll_list`]). It also made `funcs.rs`'s `f_win_gettype`
//! "loclist" branch testable for the first time, which had been
//! explicitly noted there as untestable while the placeholder had no
//! public constructor.
//!
//! The entries are held in a `Vec<QflineT>` rather than reproducing
//! the original's `qf_next`/`qf_prev` doubly-linked list: the list
//! wholly owns its entries and only ever walks them in order, so the
//! links carry nothing the `Vec` does not, and a self-referential
//! structure would need raw pointers to express. `qf_count` becomes a
//! method over that vector rather than a separately-maintained field.
//!
//! Also translated: the entry-navigation trio [`get_next_valid_entry`]/
//! [`get_prev_valid_entry`]/[`get_nth_entry`], plus
//! [`QfListT::entry_at`] for the original's 1-BASED `qf_index`
//! numbering. These return an index rather than an entry pointer,
//! which is the same information over a `Vec`. Their `qf_next ==
//! NULL`/`qf_prev == NULL` guards are defensive against a chain
//! shorter than `qf_count`; over a `Vec` the index bound is exact, so
//! they drop out. `get_nth_valid_entry`/`qf_get_entry` stay deferred -
//! they report `e_no_more_items` through `emsg`.
//!
//! Also translated: [`qf_alloc_stack`]/[`qf_free_list_stack_items`].
//! `qf_alloc_stack` returns an owned stack rather than the original's
//! pointer to one of two places (the `ql_info_actual` singleton for a
//! quickfix stack, a fresh allocation for a location list) - that
//! choice belongs to `qf_init_stack`/the per-window `w_llist` fields,
//! neither translated yet. The refcount difference between the two IS
//! kept, since it is part of the value rather than of where it lives.
//!
//! Also translated: the stack-management pair [`qf_pop_stack`]/
//! [`qf_new_list`], plus the `last_qf_id` counter handing out list
//! ids. The original works inside a fixed `qf_maxcount` array, shifting
//! entries down and zeroing the vacated top slot rather than
//! shortening the allocation, so `qf_pop_stack` pushes a default entry
//! after removing the first to keep that same shape - `qf_listcount`,
//! not the vector's length, is what tracks how many lists are live.
//!
//! Also translated: [`qf_free_items`]/[`qf_free`]/[`qf_id2nr`]. Note
//! that `qf_free_items`' original walks the linked list wrapped in two
//! defensive workarounds - a `stop` flag catching a node whose
//! `qf_next` points at itself, and a `qf_count = 1` fixup for
//! `qf_count` disagreeing with the real chain (carrying its own
//! `TODO(vim)`). Neither hazard can arise over a `Vec`, so both drop
//! out rather than being reproduced as dead defensive code.
//!
//! Also translated: [`qf_store_title`]/[`qf_cmdtitle`]. Cross-checking
//! against a real `nvim` showed the original's own doc comment on
//! `qf_store_title` ("Prepends ':' to the title") is stale - the body
//! does not, and `setqflist(.., {'title': 'mytitle'})` reports back
//! `mytitle` unchanged. The `':'` comes from `qf_cmdtitle`, which
//! callers pass a command through first (`cexpr! []` yields the title
//! `:cexpr! []`). `qf_cmdtitle` returns an owned buffer instead of the
//! original's shared `static char qftitle_str[IOSIZE]`, but keeps the
//! `IOSIZE` truncation, which is observable in the title.
//!
//! Also translated: the shared `qfga` scratch grow-array and its
//! [`qfga_get`]/[`qfga_clear`] pair, which are self-contained buffer
//! management with no dependency on the parsing machinery. The
//! original's `static bool initialized` guarding a one-time `ga_init`
//! becomes a [`std::sync::LazyLock`], so that flag has no counterpart.
//!
//! Deferred: everything else in the file - the errorformat parsing
//! machinery (`efm_T`, `qfstate_T`, `qffields_T`, and the
//! `dir_stack_T` directory tracking, whose two `qf_list_T` fields are
//! omitted here for that reason), the quickfix window/buffer UI, and
//! error navigation.

use crate::buffer_defs::{BufT, WinT, BUF_HAS_LL_ENTRY, BUF_HAS_QF_ENTRY};
use crate::garray_defs::GarrayT;

/// One entry of the directory stack used while parsing `'errorformat'`
/// output (`dir_stack_T`).
///
/// The original is an intrusive singly-linked list of raw pointers,
/// each owning its own `dirname`. An owned `Option<Box<..>>` chain
/// expresses the same shape while making the ownership explicit -
/// which is what lets the explicit `xfree` calls disappear below.
#[derive(Debug, Default)]
pub struct DirStackT {
    /// The next entry down the stack (`next`).
    pub next: Option<Box<DirStackT>>,
    /// The directory this entry names (`dirname`).
    pub dirname: Option<Vec<u8>>,
}

/// Pop the top entry off a directory stack (`qf_pop_dir`).
///
/// @return the directory now on top, or `None` when the stack has been
///         emptied. The original returns the new top's own `dirname`
///         pointer; this returns a copy, since the entry it points
///         into may be freed by a later pop.
///
/// The original frees the popped entry explicitly; dropping the owned
/// `Box` here is the whole of that.
pub fn qf_pop_dir(stackptr: &mut Option<Box<DirStackT>>) -> Option<Vec<u8>> {
    // Pop the top element and free it.
    if let Some(top) = stackptr.take() {
        *stackptr = top.next;
    }

    stackptr.as_ref().and_then(|d| d.dirname.clone())
}

/// Empty a directory stack completely (`qf_clean_dir_stack`).
///
/// The original walks the list freeing each entry and its `dirname`;
/// dropping the owned chain does all of that.
pub fn qf_clean_dir_stack(stackptr: &mut Option<Box<DirStackT>>) {
    // Iteratively, not by simply assigning None: a recursive drop of a
    // long chain could overflow the stack.
    let mut cur = stackptr.take();
    while let Some(mut entry) = cur {
        cur = entry.next.take();
    }
}

/// Propagate a window's `'lhistory'` to its location-list window, if
/// it has one (`qf_sync_win_to_llw`).
///
/// Only the quickfix-type window whose own `w_llist_ref` names this
/// window's location list counts, so an unrelated location-list window
/// is left alone.
///
/// # Safety
/// `pwp` must point at a live `WinT`, and `GLOBALS.firstwin`'s own
/// `w_next` chain must consist of live windows with valid buffers.
pub unsafe fn qf_sync_win_to_llw(pwp: *mut WinT) {
    // SAFETY: forwarded from this function's own safety doc.
    let llw = unsafe { (*pwp).w_llist };
    if llw.is_null() {
        return;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let lhi = unsafe { (*pwp).w_onebuf_opt.wo_lhi };

    // SAFETY: forwarded from this function's own safety doc.
    let mut wp = unsafe { crate::globals::GLOBALS.get_mut() }.firstwin;
    while !wp.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        let (llist_ref, buf) = unsafe { ((*wp).w_llist_ref, (*wp).w_buffer) };
        // SAFETY: forwarded from this function's own safety doc.
        let is_qf = crate::buffer::bt_quickfix(unsafe { buf.as_ref() });
        if std::ptr::eq(llist_ref, llw) && is_qf {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { (*wp).w_onebuf_opt.wo_lhi = lhi };
            return;
        }
        // SAFETY: forwarded from this function's own safety doc.
        wp = unsafe { (*wp).w_next };
    }
}

/// A window displaying a Vim help file in the current tabpage
/// (`qf_find_help_win`).
///
/// Hidden and unfocusable floating windows are skipped: they cannot be
/// jumped into, so they are not usable targets.
///
/// # Safety
/// `GLOBALS.firstwin` and its `w_next` chain must be valid pointers to
/// live `WinT`s, each with a valid `w_buffer`.
#[must_use]
pub unsafe fn qf_find_help_win() -> *mut WinT {
    // SAFETY: forwarded from this function's own safety doc.
    let mut wp = unsafe { crate::globals::GLOBALS.get_mut() }.firstwin;
    while !wp.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        let (buf, config) = unsafe { ((*wp).w_buffer, &(*wp).w_config) };
        // SAFETY: forwarded from this function's own safety doc.
        let is_help = crate::buffer::bt_help(unsafe { buf.as_ref() });
        if is_help && !config.hide && config.focusable {
            return wp;
        }
        // SAFETY: forwarded from this function's own safety doc.
        wp = unsafe { (*wp).w_next };
    }
    std::ptr::null_mut()
}

/// Attach location list stack `qi` to window `wp` (`win_set_loclist`).
///
/// The stack's reference count is incremented, since the window now
/// holds a reference to it.
///
/// # Safety
/// `wp` and `qi` must be valid, non-null pointers to a live `WinT`
/// and `QfInfoT`.
pub unsafe fn win_set_loclist(wp: *mut WinT, qi: *mut crate::types_defs::QfInfoT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        (*wp).w_llist = qi;
        (*qi).qf_refcount += 1;
    }
}

/// Status returned by the quickfix line-parsing helpers (`QF_FAIL`,
/// `QF_OK`, ... - an anonymous enum in the original).
pub mod qf_status {
    /// The field could not be parsed (`QF_FAIL`).
    pub const QF_FAIL: i32 = 0;
    /// The field was parsed successfully (`QF_OK`).
    pub const QF_OK: i32 = 1;
    /// There is no more input to read (`QF_END_OF_INPUT`).
    pub const QF_END_OF_INPUT: i32 = 2;
    /// Out of memory (`QF_NOMEM`).
    pub const QF_NOMEM: i32 = 3;
    /// This line should be skipped (`QF_IGNORE_LINE`).
    pub const QF_IGNORE_LINE: i32 = 4;
    /// Rescan the line with the next format (`QF_MULTISCAN`).
    pub const QF_MULTISCAN: i32 = 5;
}

/// The fields parsed out of one error line, before they become a
/// [`QflineT`] entry (`qffields_T`).
///
/// The original's four `char *` buffers become owned `Vec<u8>`s, which
/// dissolves two of its functions entirely:
///
/// - `qf_alloc_fields` pre-allocates each buffer at `CMDBUFFSIZE + 1`
///   and records that size in `errmsglen`. A `Vec` owns and grows its
///   own storage, so [`QffieldsT::default`] replaces it.
/// - `qf_free_fields` is four `xfree`s, which is exactly what
///   dropping the owned values already does, so it has no body left to
///   translate - the same reasoning that leaves `move.c`'s
///   `redraw_for_cursorline` untranslated rather than empty.
///
/// `errmsglen` is likewise absent: every one of its uses in the
/// original is buffer-capacity bookkeeping for those `xrealloc`s, never
/// data anyone reads, so the `Vec`'s own storage replaces it - the same
/// call already made for `TypedGarrayT`'s derived `ga_len`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct QffieldsT {
    /// File name for the error (`namebuf`).
    pub namebuf: Vec<u8>,
    /// Buffer number for the error (`bnr`).
    pub bnr: i32,
    /// Module name, shown instead of the file name (`module`).
    pub module: Vec<u8>,
    /// The error message itself (`errmsg`).
    pub errmsg: Vec<u8>,
    /// Line number where the error occurred (`lnum`).
    pub lnum: crate::pos_defs::LinenrT,
    /// Last line of a range, or zero (`end_lnum`).
    pub end_lnum: crate::pos_defs::LinenrT,
    /// Column where the error occurred (`col`).
    pub col: i32,
    /// Last column of a range, or zero (`end_col`).
    pub end_col: i32,
    /// Whether `col` is a visual column (`use_viscol`).
    pub use_viscol: bool,
    /// Search pattern for locating the error (`pattern`).
    pub pattern: Vec<u8>,
    /// Error number (`enr`).
    pub enr: i32,
    /// Error type: `'e'`, `'w'`, `'i'` and so on (`type`).
    pub type_: u8,
    /// Custom data attached by the caller (`user_data`), absent when
    /// the original's pointer is `NULL`.
    pub user_data: Option<crate::eval::typval_defs::TypvalT>,
    /// Whether the entry is a recognised error (`valid`).
    pub valid: bool,
}

fn qf_parse_atol_match(matched: Option<&[u8]>) -> Option<i32> {
    matched.map(|text| crate::charset::getdigits_int(text, false, 0).0)
}

/// Parses an `'errorformat'` `%n` error number
/// (`qf_parse_fmt_n`).
pub fn qf_parse_fmt_n(
    matched: Option<&[u8]>,
    fields: &mut QffieldsT,
) -> i32 {
    let Some(value) = qf_parse_atol_match(matched) else {
        return qf_status::QF_FAIL;
    };
    fields.enr = value;
    qf_status::QF_OK
}

/// Parses an `'errorformat'` `%l` line number
/// (`qf_parse_fmt_l`).
pub fn qf_parse_fmt_l(
    matched: Option<&[u8]>,
    fields: &mut QffieldsT,
) -> i32 {
    let Some(value) = qf_parse_atol_match(matched) else {
        return qf_status::QF_FAIL;
    };
    fields.lnum = value;
    qf_status::QF_OK
}

/// Parses an `'errorformat'` `%t` error-type match
/// (`qf_parse_fmt_t`).
pub fn qf_parse_fmt_t(
    matched: Option<&[u8]>,
    fields: &mut QffieldsT,
) -> i32 {
    let Some(matched) = matched else {
        return qf_status::QF_FAIL;
    };
    fields.type_ = matched.first().copied().unwrap_or(0);
    qf_status::QF_OK
}

/// Parses an `'errorformat'` `%m` message match
/// (`qf_parse_fmt_m`).
pub fn qf_parse_fmt_m(
    matched: Option<&[u8]>,
    fields: &mut QffieldsT,
) -> i32 {
    let Some(matched) = matched else {
        return qf_status::QF_FAIL;
    };
    let end = matched
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(matched.len());
    fields.errmsg.clear();
    fields.errmsg.extend_from_slice(&matched[..end]);
    fields.errmsg.push(0);
    qf_status::QF_OK
}

/// Parses an `'errorformat'` `%r` tail match
/// (`qf_parse_fmt_r`).
pub fn qf_parse_fmt_r<'a>(
    matched: Option<&'a [u8]>,
    tail: &mut Option<&'a [u8]>,
) -> i32 {
    let Some(matched) = matched else {
        return qf_status::QF_FAIL;
    };
    *tail = Some(matched);
    qf_status::QF_OK
}

/// Parses an `'errorformat'` `%p` pointer-line match
/// (`qf_parse_fmt_p`).
pub fn qf_parse_fmt_p(
    matched: Option<&[u8]>,
    fields: &mut QffieldsT,
) -> i32 {
    let Some(matched) = matched else {
        return qf_status::QF_FAIL;
    };
    fields.col = 0;
    for &byte in matched {
        fields.col += 1;
        if byte == crate::ascii_defs::TAB {
            fields.col += 7;
            fields.col -= fields.col % 8;
        }
    }
    fields.col += 1;
    fields.use_viscol = true;
    qf_status::QF_OK
}

/// Parses an `'errorformat'` `%v` visual-column number
/// (`qf_parse_fmt_v`).
pub fn qf_parse_fmt_v(
    matched: Option<&[u8]>,
    fields: &mut QffieldsT,
) -> i32 {
    let Some(matched) = matched else {
        return qf_status::QF_FAIL;
    };
    fields.col = crate::charset::getdigits_int(matched, false, 0).0;
    fields.use_viscol = true;
    qf_status::QF_OK
}

/// Parses an `'errorformat'` `%s` search-text match
/// (`qf_parse_fmt_s`).
///
/// The result is a very-nomagic anchored pattern:
/// `^\V{match}\$`, NUL-terminated and capped to `CMDBUFFSIZE`.
pub fn qf_parse_fmt_s(
    matched: Option<&[u8]>,
    fields: &mut QffieldsT,
) -> i32 {
    let Some(matched) = matched else {
        return qf_status::QF_FAIL;
    };
    let len = matched
        .len()
        .min(crate::os::os_defs::CMDBUFFSIZE - 5);
    fields.pattern.clear();
    fields.pattern.extend_from_slice(b"^\\V");
    fields.pattern.extend_from_slice(&matched[..len]);
    fields.pattern.extend_from_slice(b"\\$\0");
    qf_status::QF_OK
}

/// Parses an `'errorformat'` `%o` module-name match
/// (`qf_parse_fmt_o`) by appending it to the existing module string.
pub fn qf_parse_fmt_o(
    matched: Option<&[u8]>,
    fields: &mut QffieldsT,
) -> i32 {
    let Some(matched) = matched else {
        return qf_status::QF_FAIL;
    };
    let existing_end = fields
        .module
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(fields.module.len());
    fields.module.truncate(existing_end);
    let match_end = matched
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(matched.len());
    let max_content = crate::os::os_defs::CMDBUFFSIZE - 1;
    let available = max_content.saturating_sub(fields.module.len());
    fields
        .module
        .extend_from_slice(&matched[..match_end.min(available)]);
    fields.module.push(0);
    qf_status::QF_OK
}

/// Copy a line that matched no error format into the message field
/// (`copy_nonerror_line`).
///
/// Always succeeds, returning `QF_OK`.
///
/// The original grows `errmsg` when the line does not fit and then
/// `xstrlcpy`s into it; assigning the `Vec` does both. Note the copy
/// is NUL-scanned, not a verbatim `linelen` bytes: `xstrlcpy` stops at
/// the source's first NUL, so an embedded NUL truncates the message.
/// That is preserved.
pub fn copy_nonerror_line(linebuf: &[u8], linelen: usize, fields: &mut QffieldsT) -> i32 {
    let n = linelen.min(linebuf.len());
    let end = linebuf[..n].iter().position(|&b| b == 0).unwrap_or(n);

    fields.errmsg.clear();
    fields.errmsg.extend_from_slice(&linebuf[..end]);
    fields.errmsg.push(0);

    qf_status::QF_OK
}

/// Propagate a location-list window's `'lhistory'` back to the window
/// that owns the location list (`qf_sync_llw_to_win`).
///
/// The reverse direction of [`qf_sync_win_to_llw`]: that one pushes a
/// normal window's value out to its location-list window, this one
/// pulls a location-list window's value back to the normal window.
/// Note the asymmetry in how each finds its partner - this one
/// delegates to [`qf_find_win_with_loclist`], which deliberately skips
/// quickfix windows, so the value lands on the window owning the file
/// rather than on another list window.
///
/// Does nothing when no such window exists.
///
/// # Safety
/// `llw` must point at a live `WinT`. Also carries
/// [`qf_find_win_with_loclist`]'s own requirement that
/// `GLOBALS.firstwin`'s `w_next` chain consists of live windows with
/// valid buffers.
pub unsafe fn qf_sync_llw_to_win(llw: *mut WinT) {
    // SAFETY: forwarded from this function's own safety doc.
    let llist_ref = unsafe { (*llw).w_llist_ref };
    // SAFETY: forwarded from this function's own safety doc.
    let wp = unsafe { qf_find_win_with_loclist(llist_ref) };
    if wp.is_null() {
        return;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let lhi = unsafe { (*llw).w_onebuf_opt.wo_lhi };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*wp).w_onebuf_opt.wo_lhi = lhi };
}

/// Find a NON-quickfix window in the current tabpage using location
/// list stack `ll` (`qf_find_win_with_loclist`), or null if there is
/// none.
///
/// Quickfix/location-list windows are deliberately skipped: this
/// looks for the window whose file the list belongs to, not the
/// window displaying the list itself.
///
/// # Safety
/// `GLOBALS.firstwin` and its `w_next` chain must be valid pointers to
/// live `WinT`s, each with a valid `w_buffer`.
#[must_use]
pub unsafe fn qf_find_win_with_loclist(ll: *const crate::types_defs::QfInfoT) -> *mut WinT {
    // SAFETY: forwarded from this function's own safety doc.
    let mut wp = unsafe { crate::globals::GLOBALS.get_mut() }.firstwin;
    while !wp.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        let (llist, buf) = unsafe { ((*wp).w_llist, (*wp).w_buffer) };
        // SAFETY: forwarded from this function's own safety doc.
        if std::ptr::eq(llist, ll) && !crate::buffer::bt_quickfix(unsafe { buf.as_ref() }) {
            return wp;
        }
        // SAFETY: forwarded from this function's own safety doc.
        wp = unsafe { (*wp).w_next };
    }
    std::ptr::null_mut()
}

/// Sentinel for "no quickfix list index" (`INVALID_QFIDX`).
pub const INVALID_QFIDX: i32 = -1;
/// Sentinel for "no quickfix window buffer" (`INVALID_QFBUFNR`).
pub const INVALID_QFBUFNR: i32 = 0;

/// Quickfix list type (`qfltype_T`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QfltypeT {
    /// Quickfix list - global list (`QFLT_QUICKFIX`).
    #[default]
    Quickfix,
    /// Location list - per window list (`QFLT_LOCATION`).
    Location,
    /// Temporary list used by `getqflist()`/`getloclist()`
    /// (`QFLT_INTERNAL`).
    Internal,
}

/// One error entry in a quickfix/location list (`qfline_T`).
///
/// The original threads these on a `qf_next`/`qf_prev` doubly-linked
/// list that its owning [`QfListT`] wholly owns and only ever walks in
/// order, so the links carry no information the containing `Vec` does
/// not already provide. Storing the entries in a `Vec` and addressing
/// the current one by index is the direct equivalent, and avoids a
/// self-referential structure that Rust cannot express without raw
/// pointers - the same reasoning already applied to `cmdhist.c`'s own
/// history ring.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct QflineT {
    /// Line number where the error occurred (`qf_lnum`).
    pub qf_lnum: crate::pos_defs::LinenrT,
    /// Line number when the error has a range, or zero (`qf_end_lnum`).
    pub qf_end_lnum: crate::pos_defs::LinenrT,
    /// File number for the line (`qf_fnum`).
    pub qf_fnum: i32,
    /// Column where the error occurred (`qf_col`).
    pub qf_col: i32,
    /// Column when the error has a range, or zero (`qf_end_col`).
    pub qf_end_col: i32,
    /// Error number (`qf_nr`).
    pub qf_nr: i32,
    /// Module name for this error (`qf_module`).
    pub qf_module: Option<Vec<u8>>,
    /// Different filename if there are hard links (`qf_fname`).
    pub qf_fname: Option<Vec<u8>>,
    /// Search pattern for the error (`qf_pattern`).
    pub qf_pattern: Option<Vec<u8>>,
    /// Description of the error (`qf_text`).
    pub qf_text: Option<Vec<u8>>,
    /// Whether `qf_col`/`qf_end_col` are screen columns (`qf_viscol`).
    pub qf_viscol: bool,
    /// Whether this line has been deleted (`qf_cleared`).
    pub qf_cleared: bool,
    /// Type of the error (mostly `b'E'`); 1 for `:helpgrep` (`qf_type`).
    pub qf_type: u8,
    /// Custom user data associated with this item (`qf_user_data`).
    pub qf_user_data: crate::eval::typval_defs::TypvalT,
    /// Whether a valid error message was detected (`qf_valid`).
    pub qf_valid: bool,
}

/// One quickfix/location list (`qf_list_T`).
///
/// Usually holds one or more entries, but an empty list can be created
/// by `setqflist()`/`setloclist()` with only a title and/or context,
/// with entries added later.
#[derive(Debug, Default)]
pub struct QfListT {
    /// Unique identifier for this list (`qf_id`).
    pub qf_id: u32,
    /// Whether this is a quickfix or location list (`qfl_type`).
    pub qfl_type: QfltypeT,
    /// The error entries themselves - the original's `qf_start`/
    /// `qf_last` linked list (see [`QflineT`]). `qf_count` is this
    /// vector's own length rather than a separate field.
    pub qf_entries: Vec<QflineT>,
    /// Current 1-based index into `qf_entries` - the original's
    /// `qf_index`, which its `qf_ptr` always tracks (`qf_index`).
    pub qf_index: i32,
    /// Whether not a single valid entry was found (`qf_nonevalid`).
    pub qf_nonevalid: bool,
    /// Whether at least one item has user data attached
    /// (`qf_has_user_data`).
    pub qf_has_user_data: bool,
    /// Title derived from the command that created the list, or set by
    /// `setqflist` (`qf_title`).
    pub qf_title: Option<Vec<u8>>,
    /// Context set by `setqflist`/`setloclist` (`qf_ctx`).
    pub qf_ctx: Option<Box<crate::eval::typval_defs::TypvalT>>,
    /// `'quickfixtextfunc'` callback (`qf_qftf_cb`).
    pub qf_qftf_cb: crate::eval::typval_defs::Callback,
    /// Directory being parsed into (`qf_directory`).
    pub qf_directory: Option<Vec<u8>>,
    /// File currently being parsed (`qf_currfile`).
    pub qf_currfile: Option<Vec<u8>>,
    /// Whether the errorformat is multi-line (`qf_multiline`).
    pub qf_multiline: bool,
    /// Multi-line parse state (`qf_multiignore`).
    pub qf_multiignore: bool,
    /// Multi-line parse state (`qf_multiscan`).
    pub qf_multiscan: bool,
    /// Changed-tick for this list (`qf_changedtick`).
    pub qf_changedtick: i32,
}

impl QfListT {
    /// Number of errors in this list (`qf_count`), which the original
    /// tracks in its own field alongside the linked list.
    #[must_use]
    pub fn qf_count(&self) -> i32 {
        i32::try_from(self.qf_entries.len()).unwrap_or(i32::MAX)
    }

    /// The entry at a 1-BASED index, as the original's `qf_index`
    /// numbering uses (quickfix entry numbers start at 1).
    #[must_use]
    pub fn entry_at(&self, idx: i32) -> Option<&QflineT> {
        if idx < 1 {
            return None;
        }
        self.qf_entries.get(usize::try_from(idx - 1).ok()?)
    }
}

/// Length of the leading `'errorformat'` part in `efm`, up to (but not
/// including) the separating comma (`efm_option_part_len`).
///
/// A backslash escapes the byte after it, so an escaped comma does NOT
/// terminate the part. A trailing backslash at the very end does not
/// escape past the string, matching the original's own `efm[len + 1]
/// != NUL` guard.
///
/// The original relies on the C string's NUL terminator; running out
/// of the slice ends the scan the same way here.
#[must_use]
pub fn efm_option_part_len(efm: &[u8]) -> usize {
    let mut len = 0usize;
    while let Some(&c) = efm.get(len) {
        if c == crate::ascii_defs::NUL || c == b',' {
            break;
        }
        if c == b'\\' && !matches!(efm.get(len + 1), None | Some(&crate::ascii_defs::NUL)) {
            len += 1;
        }
        len += 1;
    }
    len
}

/// Step to the next valid entry at or after the current one
/// (`get_next_valid_entry`), returning its 1-based index, or `None` if
/// there is none.
///
/// Always advances at least once, then keeps going while the entry is
/// invalid - unless the list has no valid entries at all, in which
/// case every entry counts. [`crate::vim_defs::Direction::ForwardFile`]
/// additionally skips entries in the file it started from.
///
/// The original also tests `qf_ptr->qf_next == NULL` on each step,
/// which is defensive against a chain shorter than `qf_count`; over a
/// `Vec` the index bound alone is exact, so that check drops out.
#[must_use]
pub fn get_next_valid_entry(
    qfl: &QfListT,
    qf_index: i32,
    dir: crate::vim_defs::Direction,
) -> Option<i32> {
    let old_fnum = qfl.entry_at(qf_index).map(|e| e.qf_fnum);
    let mut idx = qf_index;
    loop {
        if idx >= qfl.qf_count() {
            return None;
        }
        idx += 1;
        let entry = qfl.entry_at(idx)?;
        let skip_invalid = !qfl.qf_nonevalid && !entry.qf_valid;
        let skip_same_file = dir == crate::vim_defs::Direction::ForwardFile
            && Some(entry.qf_fnum) == old_fnum;
        if !skip_invalid && !skip_same_file {
            return Some(idx);
        }
    }
}

/// Step to the previous valid entry (`get_prev_valid_entry`), the
/// mirror of [`get_next_valid_entry`], stopping at the first entry.
#[must_use]
pub fn get_prev_valid_entry(
    qfl: &QfListT,
    qf_index: i32,
    dir: crate::vim_defs::Direction,
) -> Option<i32> {
    let old_fnum = qfl.entry_at(qf_index).map(|e| e.qf_fnum);
    let mut idx = qf_index;
    loop {
        if idx <= 1 {
            return None;
        }
        idx -= 1;
        let entry = qfl.entry_at(idx)?;
        let skip_invalid = !qfl.qf_nonevalid && !entry.qf_valid;
        let skip_same_file = dir == crate::vim_defs::Direction::BackwardFile
            && Some(entry.qf_fnum) == old_fnum;
        if !skip_invalid && !skip_same_file {
            return Some(idx);
        }
    }
}

/// Move to entry number `errornr` (`get_nth_entry`), returning the
/// 1-based index actually reached.
///
/// The original walks the chain from the current entry toward
/// `errornr`, stopping at either end, so an out-of-range request
/// clamps to the nearest end rather than failing.
#[must_use]
pub fn get_nth_entry(qfl: &QfListT, errornr: i32) -> i32 {
    let mut idx = qfl.qf_index;
    while errornr < idx && idx > 1 {
        idx -= 1;
    }
    while errornr > idx && idx < qfl.qf_count() {
        idx += 1;
    }
    idx
}

/// Returns whether the specified quickfix/location stack is empty
/// (`qf_stack_empty`).
///
/// `None` stands for the original's own `qi == NULL` case.
#[must_use]
pub fn qf_stack_empty(qi: Option<&crate::types_defs::QfInfoT>) -> bool {
    qi.is_none_or(|qi| qi.qf_listcount <= 0)
}

/// Returns whether the specified quickfix/location list is empty
/// (`qf_list_empty`).
#[must_use]
pub fn qf_list_empty(qfl: Option<&QfListT>) -> bool {
    qfl.is_none_or(|qfl| qfl.qf_count() <= 0)
}

/// Value used for the `"idx"` quickfix-list property
/// (`qf_getprop_idx`).
///
/// The original immediately writes this number to a dictionary; the
/// value selection is returned directly here. A nonzero requested
/// index wins. Otherwise the current index is used, except an empty
/// list always reports zero.
#[must_use]
pub fn qf_getprop_idx(qfl: &QfListT, eidx: i32) -> i32 {
    if eidx != 0 {
        return eidx;
    }
    if qf_list_empty(Some(qfl)) {
        0
    } else {
        qfl.qf_index
    }
}

/// Adds the quickfix list title to `retdict` (`qf_getprop_title`).
pub fn qf_getprop_title(
    qfl: &QfListT,
    retdict: &mut crate::eval::typval_defs::DictT,
) -> i32 {
    crate::eval::typval::tv_dict_add_str(
        retdict,
        b"title",
        qfl.qf_title.as_deref(),
    )
}

/// Adds the file-window id associated with a location-list window to
/// `retdict` (`qf_getprop_filewinid`).
///
/// Non-location-list windows and stacks without an associated file
/// window report zero.
///
/// # Safety
/// The current window chain and all supplied pointers must remain
/// valid while [`qf_find_win_with_loclist`] walks them.
pub unsafe fn qf_getprop_filewinid(
    wp: Option<&crate::buffer_defs::WinT>,
    qi: *const crate::types_defs::QfInfoT,
    retdict: &mut crate::eval::typval_defs::DictT,
) -> i32 {
    let mut winid = 0;
    if wp.is_some_and(is_ll_window) {
        // SAFETY: forwarded from this function's own safety doc.
        let ll_wp = unsafe { qf_find_win_with_loclist(qi) };
        if !ll_wp.is_null() {
            winid = unsafe { (*ll_wp).handle };
        }
    }
    crate::eval::typval::tv_dict_add_nr(
        retdict,
        b"filewinid",
        i64::from(winid),
    )
}

/// Returns whether the list is non-empty AND has valid entries
/// (`qf_list_has_valid_entries`).
#[must_use]
pub fn qf_list_has_valid_entries(qfl: &QfListT) -> bool {
    !qf_list_empty(Some(qfl)) && !qfl.qf_nonevalid
}

/// Return the list at `idx` in the specified quickfix stack
/// (`qf_get_list`).
///
/// The original indexes `qi->qf_lists[idx]` with no bounds check at
/// all; returning `Option` keeps an out-of-range index from panicking
/// where the original would simply read past the array.
#[must_use]
pub fn qf_get_list(qi: &crate::types_defs::QfInfoT, idx: i32) -> Option<&QfListT> {
    usize::try_from(idx).ok().and_then(|idx| qi.qf_lists.get(idx))
}

/// Return the current list in the specified quickfix stack
/// (`qf_get_curlist`).
#[must_use]
pub fn qf_get_curlist(qi: &crate::types_defs::QfInfoT) -> Option<&QfListT> {
    qf_get_list(qi, qi.qf_curlist)
}

/// Whether `qi` is a quickfix (not location) stack (`IS_QF_STACK`).
#[must_use]
pub fn is_qf_stack(qi: &crate::types_defs::QfInfoT) -> bool {
    qi.qfl_type == QfltypeT::Quickfix
}

/// Whether `qi` is a location list stack (`IS_LL_STACK`).
#[must_use]
pub fn is_ll_stack(qi: &crate::types_defs::QfInfoT) -> bool {
    qi.qfl_type == QfltypeT::Location
}

/// Whether `qfl` is a quickfix (not location) list (`IS_QF_LIST`).
#[must_use]
pub fn is_qf_list(qfl: &QfListT) -> bool {
    qfl.qfl_type == QfltypeT::Quickfix
}

/// Whether `qfl` is a location list (`IS_LL_LIST`).
#[must_use]
pub fn is_ll_list(qfl: &QfListT) -> bool {
    qfl.qfl_type == QfltypeT::Location
}

/// Whether `wp` is a location-list window (`IS_LL_WINDOW`).
///
/// Such a window displays a quickfix-type buffer and carries a
/// non-null reference to the location-list stack it represents.
#[must_use]
pub fn is_ll_window(wp: &crate::buffer_defs::WinT) -> bool {
    // SAFETY: a null buffer simply fails `bt_quickfix`; otherwise the
    // caller's live window guarantees its buffer remains live.
    crate::buffer::bt_quickfix(unsafe { wp.w_buffer.as_ref() })
        && !wp.w_llist_ref.is_null()
}

/// Whether `win` displays the specified quickfix/location-list stack
/// (`is_qf_win`).
///
/// A quickfix-stack window has a null `w_llist_ref`; a location-list
/// window names its stack through that field.
///
/// # Safety
/// `qi` must remain live for the duration of the call. The buffer
/// pointer in `win`, when non-null, must point to a live `BufT`.
#[must_use]
pub unsafe fn is_qf_win(
    win: &crate::buffer_defs::WinT,
    qi: *const crate::types_defs::QfInfoT,
) -> bool {
    if qi.is_null() {
        return false;
    }
    // SAFETY: forwarded from this function's own safety doc.
    if !unsafe { crate::buffer::buf_valid(win.w_buffer) }
        || !crate::buffer::bt_quickfix(unsafe { win.w_buffer.as_ref() })
    {
        return false;
    }
    // SAFETY: `qi` is non-null and live by this function's contract.
    let qi_ref = unsafe { &*qi };
    (is_qf_stack(qi_ref) && win.w_llist_ref.is_null())
        || (is_ll_stack(qi_ref) && std::ptr::eq(win.w_llist_ref, qi.cast_mut()))
}

/// Finds the window in the current tabpage displaying stack `qi`
/// (`qf_find_win`), or null when none does.
///
/// # Safety
/// The current tabpage's `firstwin` chain must be valid and acyclic,
/// and `qi` plus every buffer referenced by the chain must remain
/// live during the walk.
#[must_use]
pub unsafe fn qf_find_win(
    qi: *const crate::types_defs::QfInfoT,
) -> *mut crate::buffer_defs::WinT {
    // SAFETY: forwarded from this function's own safety doc.
    let mut win = unsafe { crate::globals::GLOBALS.get_mut() }.firstwin;
    while !win.is_null() {
        // SAFETY: the window and its successor are valid by contract.
        if unsafe { is_qf_win(&*win, qi) } {
            return win;
        }
        win = unsafe { (*win).w_next };
    }
    std::ptr::null_mut()
}

/// Returns the current tabpage's window id for quickfix/location stack
/// `qi` (`qf_winid`), or zero when no such window is open.
///
/// # Safety
/// Same pointer and window-chain requirements as [`qf_find_win`].
#[must_use]
pub unsafe fn qf_winid(qi: *const crate::types_defs::QfInfoT) -> i32 {
    if qi.is_null() {
        return 0;
    }
    // SAFETY: forwarded from this function's own safety doc.
    let win = unsafe { qf_find_win(qi) };
    if win.is_null() {
        0
    } else {
        unsafe { (*win).handle }
    }
}

/// Buffer number used by a quickfix/location-list window
/// (`qf_getprop_qfbufnr`), or zero when the stack has no live buffer.
///
/// The original immediately writes this value to a dictionary; the
/// representation-independent value is returned directly here.
///
/// # Safety
/// Reads the global buffer list through [`crate::buffer::buflist_findnr`].
#[must_use]
pub unsafe fn qf_getprop_qfbufnr(
    qi: Option<&crate::types_defs::QfInfoT>,
) -> i32 {
    let Some(qi) = qi else {
        return 0;
    };
    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { crate::buffer::buflist_findnr(qi.qf_bufnr) }.is_null() {
        0
    } else {
        qi.qf_bufnr
    }
}

/// Free every list in the stack, but not the stack itself
/// (`qf_free_list_stack_items`).
///
/// Only the first `qf_listcount` lists are live, so slots beyond that
/// are left alone - they are already cleared.
pub fn qf_free_list_stack_items(qi: &mut crate::types_defs::QfInfoT) {
    let live = usize::try_from(qi.qf_listcount).unwrap_or(0).min(qi.qf_lists.len());
    for qfl in &mut qi.qf_lists[..live] {
        qf_free(qfl);
    }
}

/// Build a new quickfix/location list stack holding up to `n` lists
/// (`qf_alloc_stack`).
///
/// Returns an owned stack. The original instead returns a POINTER to
/// one of two places: the file-static `ql_info_actual` singleton for a
/// quickfix stack, or a fresh allocation whose `qf_refcount` starts at
/// one for a location list. That choice is a storage decision
/// belonging to the caller - [`qf_init_stack`] now makes it for the
/// quickfix stack by installing the result into `QL_INFO`, while the
/// location-list side still awaits the per-window `w_llist` wiring.
/// The refcount difference between the two IS preserved here, since it
/// is part of the returned value rather than of where it lives.
#[must_use]
pub fn qf_alloc_stack(qfltype: QfltypeT, n: i32) -> crate::types_defs::QfInfoT {
    let count = usize::try_from(n).unwrap_or(0);
    crate::types_defs::QfInfoT {
        // Only a location list stack is reference-counted; the
        // quickfix one is a static singleton in the original.
        qf_refcount: i32::from(qfltype != QfltypeT::Quickfix),
        qf_listcount: 0,
        qf_curlist: 0,
        qf_maxcount: n,
        qf_lists: (0..count).map(|_| QfListT::default()).collect(),
        qfl_type: qfltype,
        qf_bufnr: INVALID_QFBUFNR,
    }
}

/// `ql_info_actual` - the global quickfix list stack.
///
/// The original keeps a file-static struct plus a `ql_info` pointer
/// that is null until `qf_init_stack` runs, so "not yet initialized"
/// is distinguishable from "initialized but empty". `Option` models
/// that pointer directly.
static QL_INFO: crate::globals::GlobalCell<Option<crate::types_defs::QfInfoT>> =
    crate::globals::GlobalCell::new(None);

/// Initialize the global quickfix stack from `'chistory'`
/// (`qf_init_stack`).
///
/// # Safety
/// Touches `QL_INFO` and reads `crate::option_vars::OPTION_VARS`.
pub unsafe fn qf_init_stack() {
    // SAFETY: forwarded from this function's own safety doc.
    let p_chi = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_chi;
    let stack = qf_alloc_stack(QfltypeT::Quickfix, i32::try_from(p_chi).unwrap_or(0));
    // SAFETY: forwarded from this function's own safety doc.
    *unsafe { QL_INFO.get_mut() } = Some(stack);
}

/// The global quickfix stack's own window buffer number
/// (`qf_stack_get_bufnr`).
///
/// # Safety
/// Touches `QL_INFO`.
///
/// # Panics
/// If the global stack has not been initialized yet, matching the
/// original's own `assert(ql_info != NULL)`.
#[must_use]
pub unsafe fn qf_stack_get_bufnr() -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { QL_INFO.get_mut() }
        .as_ref()
        .expect("qf_stack_get_bufnr: the global quickfix stack is not initialized")
        .qf_bufnr
}

/// Returns the current quickfix entry number for window `wp`
/// (`qf_current_entry`).
///
/// A location-list window uses the stack in `w_llist_ref`; every
/// other window uses the global quickfix stack.
///
/// # Safety
/// The global stack must be initialized. Any buffer/list pointers in
/// `wp` must remain valid for the duration of the call.
///
/// # Panics
/// Panics when the selected stack or its current list does not exist,
/// matching the original's assertions/invariants.
#[must_use]
pub unsafe fn qf_current_entry(
    wp: &crate::buffer_defs::WinT,
) -> crate::pos_defs::LinenrT {
    let qi = if is_ll_window(wp) {
        // `is_ll_window` proved the reference non-null.
        unsafe { &*wp.w_llist_ref }
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { QL_INFO.get_mut() }
            .as_ref()
            .expect("qf_current_entry: the global quickfix stack is not initialized")
    };
    qf_get_curlist(qi)
        .expect("qf_current_entry: the selected stack has no current list")
        .qf_index
}

/// Whether the quickfix/location list with id `qf_id` still exists
/// (`qflist_valid`).
///
/// Used after running autocommands, which may have freed the list out
/// from under a command in progress. With `wp` null the global
/// quickfix stack is checked; otherwise the window's own location list
/// stack is, and a window that no longer exists fails immediately.
///
/// # Safety
/// `wp`, if non-null, must be a pointer that
/// [`crate::window::win_valid`] can safely inspect; touches `QL_INFO`.
#[must_use]
pub unsafe fn qflist_valid(wp: *const WinT, qf_id: u32) -> bool {
    if wp.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        let ql = unsafe { QL_INFO.get_mut() };
        return ql.as_ref().is_some_and(|qi| stack_has_list_id(qi, qf_id));
    }

    // SAFETY: forwarded from this function's own safety doc.
    if !unsafe { crate::window::win_valid(wp) } {
        return false;
    }
    // SAFETY: forwarded from this function's own safety doc; `wp` was
    // just confirmed to be a live window.
    let qi = unsafe { (*wp).w_llist };
    if qi.is_null() {
        return false;
    }
    // SAFETY: forwarded from this function's own safety doc.
    stack_has_list_id(unsafe { &*qi }, qf_id)
}

/// Whether any of the LIVE lists on `qi` has id `qf_id`.
///
/// Only the first `qf_listcount` lists are live; slots beyond that are
/// stale leftovers and must not match, which is why this cannot simply
/// scan the whole vector.
fn stack_has_list_id(qi: &crate::types_defs::QfInfoT, qf_id: u32) -> bool {
    let live = usize::try_from(qi.qf_listcount).unwrap_or(0).min(qi.qf_lists.len());
    qi.qf_lists[..live].iter().any(|qfl| qfl.qf_id == qf_id)
}

/// Counter handing out the unique id for each new list (`last_qf_id`).
static LAST_QF_ID: crate::globals::GlobalCell<u32> = crate::globals::GlobalCell::new(0);

/// Drop the oldest list off the bottom of the stack (`qf_pop_stack`).
///
/// `adjust` also fixes up `qf_listcount`/`qf_curlist` so the current
/// list stays pointed at the same list, or at the newest one if it was
/// the one removed.
///
/// The original shifts the entries down inside a fixed `qf_maxcount`
/// array and zeroes the now-unused top slot, so the allocation's own
/// length never changes and `qf_listcount` alone tracks how many are
/// live. Pushing a default entry after removing the first reproduces
/// exactly that.
pub fn qf_pop_stack(qi: &mut crate::types_defs::QfInfoT, adjust: bool) {
    if qi.qf_lists.is_empty() {
        return;
    }
    qf_free(&mut qi.qf_lists[0]);
    qi.qf_lists.remove(0);
    qi.qf_lists.push(QfListT::default());

    if adjust {
        qi.qf_listcount -= 1;
        if qi.qf_curlist == 0 {
            qi.qf_curlist = qi.qf_listcount - 1;
        } else {
            qi.qf_curlist -= 1;
        }
    }
}

/// Prepare a new, empty list at the top of the stack (`qf_new_list`).
///
/// Any lists above the current one are freed first, so that browsing
/// back and then starting a new list replaces the abandoned branch -
/// what makes `:grep` navigable in a tree-like way. When the stack is
/// already at `qf_maxcount`, the oldest list is dropped instead.
///
/// # Safety
/// Must not run concurrently with any other access to `LAST_QF_ID`.
pub unsafe fn qf_new_list(qi: &mut crate::types_defs::QfInfoT, qf_title: Option<&[u8]>) {
    // Delete any lists beyond the current entry.
    while qi.qf_listcount > qi.qf_curlist + 1 {
        qi.qf_listcount -= 1;
        if let Some(qfl) = usize::try_from(qi.qf_listcount).ok().and_then(|i| qi.qf_lists.get_mut(i))
        {
            qf_free(qfl);
        }
    }

    if qi.qf_listcount == qi.qf_maxcount {
        qf_pop_stack(qi, false);
        qi.qf_curlist = qi.qf_listcount - 1;
    } else {
        qi.qf_curlist = qi.qf_listcount;
        qi.qf_listcount += 1;
    }

    // SAFETY: forwarded from this function's own safety doc.
    let id = unsafe { LAST_QF_ID.get_mut() };
    *id += 1;
    let new_id = *id;

    let qfl_type = qi.qfl_type;
    let Some(qfl) = usize::try_from(qi.qf_curlist).ok().and_then(|i| qi.qf_lists.get_mut(i)) else {
        return;
    };
    // CLEAR_POINTER: the slot is reset in full before being reused.
    *qfl = QfListT::default();
    qf_store_title(qfl, qf_title);
    qfl.qfl_type = qfl_type;
    qfl.qf_id = new_id;
    qfl.qf_has_user_data = false;
}

/// Free all the entries in a quickfix list (`qf_free_items`).
///
/// The context and title are deliberately left alone; [`qf_free`]
/// clears those.
///
/// The original walks the `qf_start` linked list freeing each node,
/// wrapped in two defensive workarounds that have no counterpart
/// here: a `stop` flag detecting a node whose `qf_next` points at
/// itself, and a `qf_count = 1` fixup for `qf_count` disagreeing with
/// the actual chain (carrying its own `TODO(vim)`). Neither hazard can
/// arise with a `Vec` - it cannot be circular, and `qf_count` is
/// derived from its length rather than tracked separately - so the
/// whole loop is just clearing the vector.
pub fn qf_free_items(qfl: &mut QfListT) {
    qfl.qf_entries.clear();
    qfl.qf_index = 0;
    qfl.qf_nonevalid = true;
    qfl.qf_directory = None;
    qfl.qf_currfile = None;
    qfl.qf_multiline = false;
    qfl.qf_multiignore = false;
    qfl.qf_multiscan = false;
}

/// Free a quickfix list entirely (`qf_free`): its entries, plus the
/// associated context, title and callback that [`qf_free_items`]
/// leaves alone.
pub fn qf_free(qfl: &mut QfListT) {
    qf_free_items(qfl);
    qfl.qf_title = None;
    qfl.qf_ctx = None;
    qfl.qf_qftf_cb = crate::eval::typval_defs::Callback::default();
    qfl.qf_id = 0;
    qfl.qf_changedtick = 0;
}

/// Marks a quickfix/location list as changed (`qf_list_changed`).
///
/// Consumers use this monotonically increasing tick to notice an
/// in-place update even when the list's identity stays the same.
pub fn qf_list_changed(qfl: &mut QfListT) {
    qfl.qf_changedtick += 1;
}

/// Find the stack index of the list with the given unique id
/// (`qf_id2nr`), or [`INVALID_QFIDX`] if there is no such list.
///
/// Searches only the first `qf_listcount` lists, as the original does
/// - entries beyond that are not live.
#[must_use]
pub fn qf_id2nr(qi: &crate::types_defs::QfInfoT, qfid: u32) -> i32 {
    let live = usize::try_from(qi.qf_listcount).unwrap_or(0).min(qi.qf_lists.len());
    for (idx, qfl) in qi.qf_lists[..live].iter().enumerate() {
        if qfl.qf_id == qfid {
            return i32::try_from(idx).unwrap_or(INVALID_QFIDX);
        }
    }
    INVALID_QFIDX
}

/// Restores list `save_qfid` as the current list
/// (`qf_restore_list`).
///
/// This is used after autocommands, which may have selected another
/// list. Returns `FAIL` without changing the selection when the saved
/// list no longer exists.
pub fn qf_restore_list(qi: &mut crate::types_defs::QfInfoT, save_qfid: u32) -> i32 {
    if qf_get_curlist(qi).is_some_and(|qfl| qfl.qf_id == save_qfid) {
        return crate::vim_defs::OK;
    }

    let curlist = qf_id2nr(qi, save_qfid);
    if curlist < 0 {
        return crate::vim_defs::FAIL;
    }
    qi.qf_curlist = curlist;
    crate::vim_defs::OK
}

/// Set the title of the specified quickfix list (`qf_store_title`).
///
/// `None` leaves the previous title cleared. The original's
/// `XFREE_CLEAR` plus `xmallocz`/`xstrlcpy` pair collapses into a
/// single assignment to an owned `Option<Vec<u8>>`, since dropping the
/// old value frees it.
///
/// Note the original's own doc comment claims this prepends `':'`, but
/// its body does not - that happens in [`qf_cmdtitle`], which callers
/// pass through first. The comment is stale in the original; this
/// follows the code.
pub fn qf_store_title(qfl: &mut QfListT, title: Option<&[u8]>) {
    qfl.qf_title = title.map(<[u8]>::to_vec);
}

/// Build a quickfix list title by prefixing `':'` to a user command
/// (`qf_cmdtitle`).
///
/// Returns an owned buffer rather than the original's shared
/// `static char qftitle_str[IOSIZE]`, matching this crate's
/// established preference for owned return values over the original's
/// shared-mutable-scratch memory model. The `IOSIZE` truncation IS
/// preserved, since it is observable in the resulting title.
#[must_use]
pub fn qf_cmdtitle(cmd: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(cmd.len() + 1);
    out.push(b':');
    out.extend_from_slice(cmd);
    // snprintf(.., IOSIZE, ..) writes at most IOSIZE-1 bytes plus its
    // own NUL terminator, which this owned buffer does not carry.
    out.truncate(crate::globals::IOSIZE - 1);
    out
}

/// Shared scratch grow-array reused across quickfix commands to cut
/// down on alloc/free churn (`qfga`).
///
/// The original pairs this with a `static bool initialized` guarding a
/// one-time `ga_init`; a [`std::sync::LazyLock`] expresses that
/// directly, so the flag has no counterpart here.
static QFGA: std::sync::LazyLock<crate::globals::GlobalCell<GarrayT>> =
    std::sync::LazyLock::new(|| {
        let mut ga = GarrayT::default();
        ga.ga_init(1, 256);
        crate::globals::GlobalCell::new(ga)
    });

/// Borrow the shared scratch buffer, reset to empty (`qfga_get`).
///
/// Retains the previously-allocated capacity, which is the whole point
/// of sharing it.
///
/// # Safety
/// Must not run concurrently with any other access to `QFGA`.
pub unsafe fn qfga_get() -> &'static mut GarrayT {
    // SAFETY: forwarded from this function's own safety doc.
    let ga = unsafe { QFGA.get_mut() };
    ga.ga_len = 0;
    ga
}

/// Release the shared scratch buffer after use (`qfga_clear`).
///
/// Frees the backing memory outright if it grew beyond 1000 bytes,
/// rather than holding a large allocation between commands; otherwise
/// just resets the length so the capacity is reused.
///
/// # Safety
/// Must not run concurrently with any other access to `QFGA`.
pub unsafe fn qfga_clear() {
    // SAFETY: forwarded from this function's own safety doc.
    let ga = unsafe { QFGA.get_mut() };
    if ga.ga_maxlen > 1000 {
        ga.ga_clear();
    } else {
        ga.ga_len = 0;
    }
}

/// Adjust quickfix/location-list error entries for changed line numbers.
///
/// `wp` is `None` to check the quickfix list, or `Some` for a potential
/// location list into `buf`.
///
/// Always returns `false` in this crate today: `buf.b_has_qf_entry`
/// can never hold either [`BUF_HAS_QF_ENTRY`]/[`BUF_HAS_LL_ENTRY`] flag
/// yet (see this module's own doc comment), so the original's own
/// first, real check (`if (!(buf->b_has_qf_entry & buf_has_flag))
/// return false;`) is unconditionally taken - the rest of the
/// original's body (walking `qi->qf_lists[]`'s real entries) is
/// genuinely unreachable and not translated.
#[must_use]
pub fn qf_mark_adjust(
    buf: &BufT,
    wp: Option<&WinT>,
    _line1: crate::pos_defs::LinenrT,
    _line2: crate::pos_defs::LinenrT,
    _amount: crate::pos_defs::LinenrT,
    _amount_after: crate::pos_defs::LinenrT,
) -> bool {
    let buf_has_flag = if wp.is_none() { BUF_HAS_QF_ENTRY } else { BUF_HAS_LL_ENTRY };
    if buf.b_has_qf_entry & buf_has_flag == 0 {
        return false;
    }
    unreachable!(
        "qf_mark_adjust's real entry-adjustment body is unreachable today: nothing in this \
         crate can set BufT.b_has_qf_entry to a nonzero value yet, see this module's own doc \
         comment"
    );
}

/// Convert `text`, replacing a newline and any immediately-following
/// whitespace/newlines with a single space, appending the result byte
/// by byte to `gap` (`qf_fmt_text`).
pub fn qf_fmt_text(gap: &mut GarrayT, text: &[u8]) {
    let mut i = 0;
    while i < text.len() {
        if text[i] == b'\n' {
            gap.ga_append(b' ');
            i += 1;
            while i < text.len() && (crate::ascii_defs::ascii_iswhite(i32::from(text[i])) || text[i] == b'\n') {
                i += 1;
            }
        } else {
            gap.ga_append(text[i]);
            i += 1;
        }
    }
}

/// Find the first window in the current tab page showing a normal
/// buffer (`qf_find_win_with_normal_buf`).
///
/// Returns null when every window shows a special buffer (quickfix,
/// help, terminal and so on).
///
/// The original's `FOR_ALL_WINDOWS_IN_TAB(wp, curtab)` resolves to a
/// walk from `GLOBALS.firstwin`, matching the same simplification
/// already established elsewhere in this crate (e.g. `buffer.rs`'s
/// `wininfo_other_tab_diff`).
///
/// # Safety
/// Touches `GLOBALS.firstwin` and walks `w_next`/`w_buffer` - the
/// same requirement as every other function that walks the window
/// list.
#[must_use]
pub unsafe fn qf_find_win_with_normal_buf() -> *mut WinT {
    // SAFETY: forwarded from this function's own safety doc.
    let mut wp = unsafe { crate::globals::GLOBALS.get_mut() }.firstwin;
    while !wp.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        let win = unsafe { &*wp };
        let buf = if win.w_buffer.is_null() {
            None
        } else {
            // SAFETY: forwarded from this function's own safety doc.
            Some(unsafe { &*win.w_buffer })
        };
        if crate::buffer::bt_normal(buf) {
            return wp;
        }
        wp = win.w_next;
    }
    std::ptr::null_mut()
}

/// Finds the first quickfix entry belonging to buffer `bnr`
/// (`qf_find_first_entry_in_buf`).
///
/// The returned index is zero-based for the containing `Vec`, while
/// `errornr` keeps the original's one-based quickfix numbering. When
/// no entry matches, `errornr` is one past the list's last entry.
///
/// # Safety
/// Reads `GLOBALS.got_int`.
#[must_use]
pub unsafe fn qf_find_first_entry_in_buf(
    qfl: &QfListT,
    bnr: i32,
    errornr: &mut i32,
) -> Option<usize> {
    for (idx, entry) in qfl.qf_entries.iter().enumerate() {
        *errornr = i32::try_from(idx + 1).unwrap_or(i32::MAX);
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { crate::globals::GLOBALS.get_mut() }.got_int {
            return None;
        }
        if entry.qf_fnum == bnr {
            return Some(idx);
        }
    }
    *errornr = qfl.qf_count().saturating_add(1);
    None
}

/// Finds the first quickfix entry after `pos` in buffer `bnr`
/// (`qf_find_entry_after_pos`).
///
/// `start_idx` names the first entry in that buffer and `errornr` is
/// its one-based quickfix number. The returned value is the matching
/// zero-based `Vec` index.
///
/// # Panics
/// Panics when `start_idx` is outside `entries`, matching the
/// original's requirement that its starting entry pointer is valid.
#[must_use]
pub fn qf_find_entry_after_pos(
    entries: &[QflineT],
    bnr: i32,
    pos: &crate::pos_defs::PosT,
    linewise: bool,
    start_idx: usize,
    errornr: &mut i32,
) -> Option<usize> {
    assert!(start_idx < entries.len(), "start_idx must name a quickfix entry");
    let mut idx = start_idx;

    if qf_entry_after_pos(&entries[idx], pos, linewise) {
        return Some(idx);
    }

    while idx + 1 < entries.len()
        && entries[idx + 1].qf_fnum == bnr
        && qf_entry_on_or_before_pos(&entries[idx + 1], pos, linewise)
    {
        idx += 1;
        *errornr += 1;
    }

    if idx + 1 >= entries.len() || entries[idx + 1].qf_fnum != bnr {
        return None;
    }

    idx += 1;
    *errornr += 1;
    Some(idx)
}

/// Finds the first quickfix entry before `pos` in buffer `bnr`
/// (`qf_find_entry_before_pos`).
///
/// The search starts at that buffer's first entry. In linewise mode,
/// when several entries share the selected line, the first one is
/// returned.
///
/// # Safety
/// Reads `GLOBALS.got_int` while rewinding a linewise match through
/// [`qf_find_first_entry_on_line`].
///
/// # Panics
/// Panics when `start_idx` is outside `entries`, matching the
/// original's valid-start-pointer precondition.
#[must_use]
pub unsafe fn qf_find_entry_before_pos(
    entries: &[QflineT],
    bnr: i32,
    pos: &crate::pos_defs::PosT,
    linewise: bool,
    start_idx: usize,
    errornr: &mut i32,
) -> Option<usize> {
    assert!(start_idx < entries.len(), "start_idx must name a quickfix entry");
    let mut idx = start_idx;

    while idx + 1 < entries.len()
        && entries[idx + 1].qf_fnum == bnr
        && qf_entry_before_pos(&entries[idx + 1], pos, linewise)
    {
        idx += 1;
        *errornr += 1;
    }

    if qf_entry_on_or_after_pos(&entries[idx], pos, linewise) {
        return None;
    }

    if linewise {
        // SAFETY: forwarded from this function's own safety doc.
        idx = unsafe { qf_find_first_entry_on_line(entries, idx, errornr) };
    }
    Some(idx)
}

/// Finds the quickfix entry in buffer `bnr` closest to `pos` in
/// `dir` (`qf_find_closest_entry`).
///
/// `errornr` receives the selected entry's one-based quickfix number.
/// `None` means either the buffer has no entry or none lies in the
/// requested direction.
///
/// # Safety
/// Reads `GLOBALS.got_int` through the constituent search helpers.
#[must_use]
pub unsafe fn qf_find_closest_entry(
    qfl: &QfListT,
    bnr: i32,
    pos: &crate::pos_defs::PosT,
    dir: crate::vim_defs::Direction,
    linewise: bool,
    errornr: &mut i32,
) -> Option<usize> {
    *errornr = 0;
    // SAFETY: forwarded from this function's own safety doc.
    let start = unsafe { qf_find_first_entry_in_buf(qfl, bnr, errornr) }?;

    if dir == crate::vim_defs::Direction::Forward {
        qf_find_entry_after_pos(
            &qfl.qf_entries,
            bnr,
            pos,
            linewise,
            start,
            errornr,
        )
    } else {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe {
            qf_find_entry_before_pos(
                &qfl.qf_entries,
                bnr,
                pos,
                linewise,
                start,
                errornr,
            )
        }
    }
}

/// Advances `errornr` by up to `n` quickfix entries below `start_idx`
/// in the same buffer (`qf_get_nth_below_entry`).
///
/// In linewise mode all entries sharing one line count as one step.
/// If the final line has no following entry in the buffer, the partial
/// within-line advance is rolled back, matching the original.
///
/// # Safety
/// Reads `GLOBALS.got_int` and calls
/// [`qf_find_last_entry_on_line`].
pub unsafe fn qf_get_nth_below_entry(
    entries: &[QflineT],
    start_idx: usize,
    mut n: crate::pos_defs::LinenrT,
    linewise: bool,
    errornr: &mut i32,
) {
    assert!(start_idx < entries.len(), "start_idx must name a quickfix entry");
    let mut idx = start_idx;

    while n > 0 {
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { crate::globals::GLOBALS.get_mut() }.got_int {
            break;
        }
        n -= 1;
        let first_errornr = *errornr;

        if linewise {
            // SAFETY: forwarded from this function's own safety doc.
            idx = unsafe { qf_find_last_entry_on_line(entries, idx, errornr) };
        }

        if idx + 1 >= entries.len()
            || entries[idx + 1].qf_fnum != entries[idx].qf_fnum
        {
            if linewise {
                *errornr = first_errornr;
            }
            break;
        }

        idx += 1;
        *errornr += 1;
    }
}

/// Moves `errornr` by up to `n` quickfix entries above `start_idx`
/// in the same buffer (`qf_get_nth_above_entry`).
///
/// In linewise mode every run sharing a line counts as one step and
/// the error number is left at that line's first entry.
///
/// # Safety
/// Reads `GLOBALS.got_int` and calls
/// [`qf_find_first_entry_on_line`].
pub unsafe fn qf_get_nth_above_entry(
    entries: &[QflineT],
    start_idx: usize,
    mut n: crate::pos_defs::LinenrT,
    linewise: bool,
    errornr: &mut i32,
) {
    assert!(start_idx < entries.len(), "start_idx must name a quickfix entry");
    let mut idx = start_idx;

    while n > 0 {
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { crate::globals::GLOBALS.get_mut() }.got_int {
            break;
        }
        n -= 1;
        if idx == 0 || entries[idx - 1].qf_fnum != entries[idx].qf_fnum {
            break;
        }
        idx -= 1;
        *errornr -= 1;
        if linewise {
            // SAFETY: forwarded from this function's own safety doc.
            idx = unsafe { qf_find_first_entry_on_line(entries, idx, errornr) };
        }
    }
}

/// Finds the `n`th quickfix entry adjacent to `pos` in buffer `bnr`
/// and direction `dir` (`qf_find_nth_adj_entry`).
///
/// Returns its one-based quickfix number, or zero when there is no
/// qualifying entry. The closest entry is number one; only the
/// remaining `n - 1` steps are delegated to the directional walker.
///
/// # Safety
/// Reads `GLOBALS.got_int` through the constituent helpers.
#[must_use]
pub unsafe fn qf_find_nth_adj_entry(
    qfl: &QfListT,
    bnr: i32,
    pos: &crate::pos_defs::PosT,
    n: crate::pos_defs::LinenrT,
    dir: crate::vim_defs::Direction,
    linewise: bool,
) -> i32 {
    let mut errornr = 0;
    // SAFETY: forwarded from this function's own safety doc.
    let Some(idx) = (unsafe {
        qf_find_closest_entry(qfl, bnr, pos, dir, linewise, &mut errornr)
    }) else {
        return 0;
    };

    let remaining = n - 1;
    if remaining > 0 {
        if dir == crate::vim_defs::Direction::Forward {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe {
                qf_get_nth_below_entry(
                    &qfl.qf_entries,
                    idx,
                    remaining,
                    linewise,
                    &mut errornr,
                );
            }
        } else {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe {
                qf_get_nth_above_entry(
                    &qfl.qf_entries,
                    idx,
                    remaining,
                    linewise,
                    &mut errornr,
                );
            }
        }
    }
    errornr
}

/// Finds the first quickfix entry on the same line as the one at
/// `idx`, updating `errornr` to match (`qf_find_first_entry_on_line`).
///
/// Assumes the entries are sorted by line number, as the original
/// does. Since [`QflineT`] entries live in a `Vec` here rather than on
/// the original's `qf_prev`/`qf_next` links, this walks indices; the
/// returned index replaces the original's returned pointer.
///
/// # Safety
/// Reads `GLOBALS.got_int`.
#[must_use]
pub unsafe fn qf_find_first_entry_on_line(
    entries: &[QflineT],
    mut idx: usize,
    errornr: &mut i32,
) -> usize {
    while idx > 0 {
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { crate::globals::GLOBALS.get_mut() }.got_int {
            break;
        }
        let cur = &entries[idx];
        let prev = &entries[idx - 1];
        if cur.qf_fnum != prev.qf_fnum || cur.qf_lnum != prev.qf_lnum {
            break;
        }
        idx -= 1;
        *errornr -= 1;
    }
    idx
}

/// Finds the last quickfix entry on the same line as the one at
/// `idx`, updating `errornr` to match (`qf_find_last_entry_on_line`).
///
/// The mirror of [`qf_find_first_entry_on_line`]; see it for how the
/// original's list links map onto indices.
///
/// # Safety
/// Reads `GLOBALS.got_int`.
#[must_use]
pub unsafe fn qf_find_last_entry_on_line(
    entries: &[QflineT],
    mut idx: usize,
    errornr: &mut i32,
) -> usize {
    while idx + 1 < entries.len() {
        // SAFETY: forwarded from this function's own safety doc.
        if unsafe { crate::globals::GLOBALS.get_mut() }.got_int {
            break;
        }
        let cur = &entries[idx];
        let next = &entries[idx + 1];
        if cur.qf_fnum != next.qf_fnum || cur.qf_lnum != next.qf_lnum {
            break;
        }
        idx += 1;
        *errornr += 1;
    }
    idx
}

/// Whether the quickfix entry `qfp` is after `pos`
/// (`qf_entry_after_pos`).
///
/// With `linewise` only the line number is considered; otherwise the
/// column breaks a tie on the same line.
#[must_use]
pub fn qf_entry_after_pos(qfp: &QflineT, pos: &crate::pos_defs::PosT, linewise: bool) -> bool {
    if linewise {
        return qfp.qf_lnum > pos.lnum;
    }
    qfp.qf_lnum > pos.lnum || (qfp.qf_lnum == pos.lnum && qfp.qf_col > pos.col)
}

/// Whether the quickfix entry `qfp` is before `pos`
/// (`qf_entry_before_pos`).
///
/// See [`qf_entry_after_pos`] for how `linewise` is treated.
#[must_use]
pub fn qf_entry_before_pos(qfp: &QflineT, pos: &crate::pos_defs::PosT, linewise: bool) -> bool {
    if linewise {
        return qfp.qf_lnum < pos.lnum;
    }
    qfp.qf_lnum < pos.lnum || (qfp.qf_lnum == pos.lnum && qfp.qf_col < pos.col)
}

/// Whether the quickfix entry `qfp` is on or after `pos`
/// (`qf_entry_on_or_after_pos`).
///
/// Note the line comparison stays strict even here: only the column
/// test is relaxed to `>=`, so an entry on the same line still has to
/// reach `pos`'s column.
#[must_use]
pub fn qf_entry_on_or_after_pos(
    qfp: &QflineT,
    pos: &crate::pos_defs::PosT,
    linewise: bool,
) -> bool {
    if linewise {
        return qfp.qf_lnum >= pos.lnum;
    }
    qfp.qf_lnum > pos.lnum || (qfp.qf_lnum == pos.lnum && qfp.qf_col >= pos.col)
}

/// Whether the quickfix entry `qfp` is on or before `pos`
/// (`qf_entry_on_or_before_pos`).
///
/// See [`qf_entry_on_or_after_pos`] for the asymmetry between the
/// line and column comparisons.
#[must_use]
pub fn qf_entry_on_or_before_pos(
    qfp: &QflineT,
    pos: &crate::pos_defs::PosT,
    linewise: bool,
) -> bool {
    if linewise {
        return qfp.qf_lnum <= pos.lnum;
    }
    qfp.qf_lnum < pos.lnum || (qfp.qf_lnum == pos.lnum && qfp.qf_col <= pos.col)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestDict(*mut crate::eval::typval_defs::DictT);

    impl TestDict {
        fn new() -> Self {
            Self(crate::eval::typval::tv_dict_alloc())
        }

        fn get(&mut self) -> &mut crate::eval::typval_defs::DictT {
            unsafe { &mut *self.0 }
        }
    }

    impl Drop for TestDict {
        fn drop(&mut self) {
            unsafe { crate::eval::typval::tv_dict_free(self.0) };
        }
    }

    // --- qf_find_first_entry_in_buf / first/last_entry_on_line ---

    fn entry_fl(fnum: i32, lnum: i32) -> QflineT {
        QflineT {
            qf_fnum: fnum,
            qf_lnum: lnum,
            ..Default::default()
        }
    }

    /// Restores `got_int` on drop, so a failing assertion cannot leave
    /// the interrupt flag set for later tests.
    struct GotIntGuard(bool);

    impl GotIntGuard {
        fn set(v: bool) -> Self {
            // SAFETY: the global state test lock is held by the caller.
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            let saved = g.got_int;
            g.got_int = v;
            Self(saved)
        }
    }

    impl Drop for GotIntGuard {
        fn drop(&mut self) {
            // SAFETY: as in `set`.
            unsafe { crate::globals::GLOBALS.get_mut() }.got_int = self.0;
        }
    }

    #[test]
    fn qf_parse_fmt_t_stores_the_first_matched_byte() {
        let mut fields = QffieldsT::default();
        assert_eq!(
            qf_parse_fmt_t(Some(b"Error"), &mut fields),
            qf_status::QF_OK
        );
        assert_eq!(fields.type_, b'E');
    }

    #[test]
    fn qf_parse_fmt_n_parses_an_atol_compatible_error_number() {
        let mut fields = QffieldsT::default();
        assert_eq!(
            qf_parse_fmt_n(Some(b"42 trailing"), &mut fields),
            qf_status::QF_OK
        );
        assert_eq!(fields.enr, 42);
        qf_parse_fmt_n(Some(b"-7"), &mut fields);
        assert_eq!(fields.enr, -7);
        qf_parse_fmt_n(Some(b"none"), &mut fields);
        assert_eq!(fields.enr, 0);
    }

    #[test]
    fn qf_parse_fmt_n_rejects_a_missing_match_without_changing_error_number() {
        let mut fields = QffieldsT {
            enr: 9,
            ..Default::default()
        };
        assert_eq!(qf_parse_fmt_n(None, &mut fields), qf_status::QF_FAIL);
        assert_eq!(fields.enr, 9);
    }

    #[test]
    fn qf_parse_fmt_l_parses_the_source_line_number() {
        let mut fields = QffieldsT::default();
        assert_eq!(
            qf_parse_fmt_l(Some(b"123:rest"), &mut fields),
            qf_status::QF_OK
        );
        assert_eq!(fields.lnum, 123);
    }

    #[test]
    fn qf_parse_fmt_l_rejects_a_missing_match_without_changing_line() {
        let mut fields = QffieldsT {
            lnum: 8,
            ..Default::default()
        };
        assert_eq!(qf_parse_fmt_l(None, &mut fields), qf_status::QF_FAIL);
        assert_eq!(fields.lnum, 8);
    }

    #[test]
    fn qf_parse_fmt_t_rejects_a_missing_match_without_changing_type() {
        let mut fields = QffieldsT {
            type_: b'W',
            ..Default::default()
        };
        assert_eq!(qf_parse_fmt_t(None, &mut fields), qf_status::QF_FAIL);
        assert_eq!(fields.type_, b'W');
    }

    #[test]
    fn qf_parse_fmt_t_treats_an_empty_match_as_nul() {
        let mut fields = QffieldsT::default();
        assert_eq!(qf_parse_fmt_t(Some(b""), &mut fields), qf_status::QF_OK);
        assert_eq!(fields.type_, 0);
    }

    #[test]
    fn qf_parse_fmt_m_replaces_the_message_and_nul_terminates_it() {
        let mut fields = QffieldsT {
            errmsg: b"old message\0".to_vec(),
            ..Default::default()
        };
        assert_eq!(
            qf_parse_fmt_m(Some(b"new message"), &mut fields),
            qf_status::QF_OK
        );
        assert_eq!(fields.errmsg, b"new message\0");
    }

    #[test]
    fn qf_parse_fmt_m_stops_at_an_embedded_nul() {
        let mut fields = QffieldsT::default();
        assert_eq!(
            qf_parse_fmt_m(Some(b"head\0tail"), &mut fields),
            qf_status::QF_OK
        );
        assert_eq!(fields.errmsg, b"head\0");
    }

    #[test]
    fn qf_parse_fmt_m_rejects_a_missing_match_without_changing_message() {
        let mut fields = QffieldsT {
            errmsg: b"keep\0".to_vec(),
            ..Default::default()
        };
        assert_eq!(qf_parse_fmt_m(None, &mut fields), qf_status::QF_FAIL);
        assert_eq!(fields.errmsg, b"keep\0");
    }

    #[test]
    fn qf_parse_fmt_r_returns_the_matched_tail_slice() {
        let source = b"remaining text";
        let mut tail = None;
        assert_eq!(
            qf_parse_fmt_r(Some(source), &mut tail),
            qf_status::QF_OK
        );
        assert_eq!(tail, Some(&source[..]));
    }

    #[test]
    fn qf_parse_fmt_r_rejects_a_missing_match_without_changing_tail() {
        let original = b"keep";
        let mut tail = Some(&original[..]);
        assert_eq!(qf_parse_fmt_r(None, &mut tail), qf_status::QF_FAIL);
        assert_eq!(tail, Some(&original[..]));
    }

    #[test]
    fn qf_parse_fmt_p_counts_pointer_characters_and_uses_one_based_column() {
        let mut fields = QffieldsT::default();
        assert_eq!(
            qf_parse_fmt_p(Some(b"   "), &mut fields),
            qf_status::QF_OK
        );
        assert_eq!(fields.col, 4);
        assert!(fields.use_viscol);
    }

    #[test]
    fn qf_parse_fmt_p_expands_tabs_to_eight_column_boundaries() {
        let mut fields = QffieldsT::default();
        assert_eq!(
            qf_parse_fmt_p(Some(b" \t"), &mut fields),
            qf_status::QF_OK
        );
        assert_eq!(fields.col, 9);
    }

    #[test]
    fn qf_parse_fmt_p_rejects_a_missing_match_without_changing_fields() {
        let mut fields = QffieldsT {
            col: 7,
            use_viscol: false,
            ..Default::default()
        };
        assert_eq!(qf_parse_fmt_p(None, &mut fields), qf_status::QF_FAIL);
        assert_eq!(fields.col, 7);
        assert!(!fields.use_viscol);
    }

    #[test]
    fn qf_parse_fmt_p_maps_an_empty_pointer_line_to_column_one() {
        let mut fields = QffieldsT::default();
        assert_eq!(qf_parse_fmt_p(Some(b""), &mut fields), qf_status::QF_OK);
        assert_eq!(fields.col, 1);
    }

    #[test]
    fn qf_parse_fmt_v_parses_a_leading_decimal_column() {
        let mut fields = QffieldsT::default();
        assert_eq!(
            qf_parse_fmt_v(Some(b"123 trailing"), &mut fields),
            qf_status::QF_OK
        );
        assert_eq!(fields.col, 123);
        assert!(fields.use_viscol);
    }

    #[test]
    fn qf_parse_fmt_v_matches_atol_for_negative_and_nondigit_values() {
        let mut fields = QffieldsT::default();
        qf_parse_fmt_v(Some(b"-7"), &mut fields);
        assert_eq!(fields.col, -7);
        qf_parse_fmt_v(Some(b"none"), &mut fields);
        assert_eq!(fields.col, 0);
    }

    #[test]
    fn qf_parse_fmt_v_rejects_a_missing_match_without_changing_fields() {
        let mut fields = QffieldsT {
            col: 7,
            use_viscol: false,
            ..Default::default()
        };
        assert_eq!(qf_parse_fmt_v(None, &mut fields), qf_status::QF_FAIL);
        assert_eq!(fields.col, 7);
        assert!(!fields.use_viscol);
    }

    #[test]
    fn qf_parse_fmt_s_builds_an_anchored_very_nomagic_pattern() {
        let mut fields = QffieldsT::default();
        assert_eq!(
            qf_parse_fmt_s(Some(b"a.b"), &mut fields),
            qf_status::QF_OK
        );
        assert_eq!(fields.pattern, b"^\\Va.b\\$\0");
    }

    #[test]
    fn qf_parse_fmt_s_handles_an_empty_match() {
        let mut fields = QffieldsT::default();
        qf_parse_fmt_s(Some(b""), &mut fields);
        assert_eq!(fields.pattern, b"^\\V\\$\0");
    }

    #[test]
    fn qf_parse_fmt_s_caps_the_match_to_the_command_buffer_limit() {
        let mut fields = QffieldsT::default();
        let matched = vec![b'x'; crate::os::os_defs::CMDBUFFSIZE + 50];
        qf_parse_fmt_s(Some(&matched), &mut fields);
        assert_eq!(
            fields.pattern.len(),
            crate::os::os_defs::CMDBUFFSIZE + 1
        );
        assert!(fields.pattern.ends_with(b"\\$\0"));
    }

    #[test]
    fn qf_parse_fmt_s_rejects_a_missing_match_without_changing_pattern() {
        let mut fields = QffieldsT {
            pattern: b"keep\0".to_vec(),
            ..Default::default()
        };
        assert_eq!(qf_parse_fmt_s(None, &mut fields), qf_status::QF_FAIL);
        assert_eq!(fields.pattern, b"keep\0");
    }

    #[test]
    fn qf_parse_fmt_o_appends_to_the_existing_module_name() {
        let mut fields = QffieldsT {
            module: b"core\0".to_vec(),
            ..Default::default()
        };
        assert_eq!(
            qf_parse_fmt_o(Some(b"::sub"), &mut fields),
            qf_status::QF_OK
        );
        assert_eq!(fields.module, b"core::sub\0");
    }

    #[test]
    fn qf_parse_fmt_o_initializes_an_empty_module_buffer() {
        let mut fields = QffieldsT::default();
        qf_parse_fmt_o(Some(b"module"), &mut fields);
        assert_eq!(fields.module, b"module\0");
    }

    #[test]
    fn qf_parse_fmt_o_caps_the_combined_module_to_cmdbuffsize() {
        let mut fields = QffieldsT {
            module: vec![b'a'; crate::os::os_defs::CMDBUFFSIZE - 5],
            ..Default::default()
        };
        qf_parse_fmt_o(Some(b"0123456789"), &mut fields);
        assert_eq!(fields.module.len(), crate::os::os_defs::CMDBUFFSIZE);
        assert_eq!(fields.module.last(), Some(&0));
    }

    #[test]
    fn qf_parse_fmt_o_rejects_a_missing_match_without_changing_module() {
        let mut fields = QffieldsT {
            module: b"keep\0".to_vec(),
            ..Default::default()
        };
        assert_eq!(qf_parse_fmt_o(None, &mut fields), qf_status::QF_FAIL);
        assert_eq!(fields.module, b"keep\0");
    }

    #[test]
    fn qf_find_first_entry_in_buf_returns_the_first_match() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = GotIntGuard::set(false);
        let qfl = QfListT {
            qf_entries: vec![
                entry_fl(1, 2),
                entry_fl(7, 3),
                entry_fl(7, 4),
            ],
            ..Default::default()
        };
        let mut errornr = 0;

        assert_eq!(
            unsafe { qf_find_first_entry_in_buf(&qfl, 7, &mut errornr) },
            Some(1)
        );
        assert_eq!(errornr, 2, "quickfix entry numbers are one-based");
    }

    #[test]
    fn qf_find_first_entry_in_buf_reports_one_past_end_on_a_miss() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = GotIntGuard::set(false);
        let qfl = QfListT {
            qf_entries: vec![entry_fl(1, 2), entry_fl(2, 3)],
            ..Default::default()
        };
        let mut errornr = 0;

        assert_eq!(
            unsafe { qf_find_first_entry_in_buf(&qfl, 9, &mut errornr) },
            None
        );
        assert_eq!(errornr, 3);
    }

    #[test]
    fn qf_find_first_entry_in_buf_stops_when_interrupted() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = GotIntGuard::set(true);
        let qfl = QfListT {
            qf_entries: vec![entry_fl(7, 2)],
            ..Default::default()
        };
        let mut errornr = 0;

        assert_eq!(
            unsafe { qf_find_first_entry_in_buf(&qfl, 7, &mut errornr) },
            None
        );
        assert_eq!(errornr, 1, "the interrupted scan stopped at its first entry");
    }

    #[test]
    fn qf_find_first_entry_in_buf_handles_an_empty_list() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = GotIntGuard::set(false);
        let mut errornr = 99;

        assert_eq!(
            unsafe {
                qf_find_first_entry_in_buf(&QfListT::default(), 7, &mut errornr)
            },
            None
        );
        assert_eq!(errornr, 1);
    }

    /// The scan spans only entries sharing BOTH file and line, and
    /// errornr moves in lockstep with the index.
    #[test]
    fn qf_find_entry_on_line_walks_the_run_of_matching_entries() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = GotIntGuard::set(false);

        let entries = vec![
            entry_fl(1, 5),
            entry_fl(1, 7),
            entry_fl(1, 7),
            entry_fl(1, 7),
            entry_fl(1, 9),
        ];

        let mut errornr = 3;
        assert_eq!(unsafe { qf_find_first_entry_on_line(&entries, 2, &mut errornr) }, 1);
        assert_eq!(errornr, 2, "errornr follows the one step taken back");

        let mut errornr = 3;
        assert_eq!(unsafe { qf_find_last_entry_on_line(&entries, 2, &mut errornr) }, 3);
        assert_eq!(errornr, 4);
    }

    /// A different file breaks the run even when the line matches -
    /// comparing only the line would wrongly join two files.
    #[test]
    fn qf_find_entry_on_line_stops_at_a_different_file() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = GotIntGuard::set(false);

        let entries = vec![entry_fl(1, 7), entry_fl(2, 7), entry_fl(3, 7)];

        let mut errornr = 2;
        assert_eq!(unsafe { qf_find_first_entry_on_line(&entries, 1, &mut errornr) }, 1);
        assert_eq!(errornr, 2, "no step taken, so errornr is unchanged");

        let mut errornr = 2;
        assert_eq!(unsafe { qf_find_last_entry_on_line(&entries, 1, &mut errornr) }, 1);
    }

    /// The ends of the list are honoured without running off either
    /// side.
    #[test]
    fn qf_find_entry_on_line_handles_the_list_ends() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = GotIntGuard::set(false);

        let entries = vec![entry_fl(1, 7), entry_fl(1, 7)];

        let mut errornr = 1;
        assert_eq!(unsafe { qf_find_first_entry_on_line(&entries, 0, &mut errornr) }, 0);
        assert_eq!(errornr, 1);

        let mut errornr = 2;
        assert_eq!(unsafe { qf_find_last_entry_on_line(&entries, 1, &mut errornr) }, 1);
        assert_eq!(errornr, 2);
    }

    /// An interrupt stops the scan where it stands, so a very long run
    /// of entries stays cancellable.
    #[test]
    fn qf_find_entry_on_line_stops_when_interrupted() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = GotIntGuard::set(true);

        let entries = vec![entry_fl(1, 7), entry_fl(1, 7), entry_fl(1, 7)];

        let mut errornr = 2;
        assert_eq!(unsafe { qf_find_first_entry_on_line(&entries, 1, &mut errornr) }, 1);
        assert_eq!(errornr, 2, "interrupted before taking a step");

        let mut errornr = 2;
        assert_eq!(unsafe { qf_find_last_entry_on_line(&entries, 1, &mut errornr) }, 1);
    }

    // --- qf_entry_*_pos ---

    fn entry_at(lnum: i32, col: i32) -> QflineT {
        QflineT {
            qf_lnum: lnum,
            qf_col: col,
            ..Default::default()
        }
    }

    fn pos_at(lnum: i32, col: i32) -> crate::pos_defs::PosT {
        crate::pos_defs::PosT {
            lnum,
            col,
            coladd: 0,
        }
    }

    fn qfl_with_entries(entries: Vec<QflineT>) -> QfListT {
        QfListT {
            qf_entries: entries,
            ..Default::default()
        }
    }

    #[test]
    fn qf_find_closest_entry_searches_forward() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = GotIntGuard::set(false);
        let qfl = qfl_with_entries(vec![
            entry_fl(7, 2),
            entry_fl(7, 5),
            entry_fl(7, 8),
        ]);
        let mut errornr = 99;

        assert_eq!(
            unsafe {
                qf_find_closest_entry(
                    &qfl,
                    7,
                    &pos_at(5, 0),
                    crate::vim_defs::Direction::Forward,
                    true,
                    &mut errornr,
                )
            },
            Some(2)
        );
        assert_eq!(errornr, 3);
    }

    #[test]
    fn qf_find_closest_entry_searches_backward() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = GotIntGuard::set(false);
        let qfl = qfl_with_entries(vec![
            entry_fl(7, 2),
            entry_fl(7, 5),
            entry_fl(7, 8),
        ]);
        let mut errornr = 99;

        assert_eq!(
            unsafe {
                qf_find_closest_entry(
                    &qfl,
                    7,
                    &pos_at(5, 0),
                    crate::vim_defs::Direction::Backward,
                    true,
                    &mut errornr,
                )
            },
            Some(0)
        );
        assert_eq!(errornr, 1);
    }

    #[test]
    fn qf_find_closest_entry_returns_none_without_that_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = GotIntGuard::set(false);
        let qfl = qfl_with_entries(vec![entry_fl(1, 2), entry_fl(2, 5)]);
        let mut errornr = 99;

        assert_eq!(
            unsafe {
                qf_find_closest_entry(
                    &qfl,
                    7,
                    &pos_at(5, 0),
                    crate::vim_defs::Direction::Forward,
                    false,
                    &mut errornr,
                )
            },
            None
        );
        assert_eq!(errornr, 3, "the initial scan ended one past the list");
    }

    #[test]
    fn qf_find_closest_entry_returns_none_at_a_directional_boundary() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = GotIntGuard::set(false);
        let qfl = qfl_with_entries(vec![entry_fl(7, 5)]);
        let mut errornr = 99;

        assert_eq!(
            unsafe {
                qf_find_closest_entry(
                    &qfl,
                    7,
                    &pos_at(5, 0),
                    crate::vim_defs::Direction::Backward,
                    true,
                    &mut errornr,
                )
            },
            None,
            "there is no entry on an earlier line"
        );
        assert_eq!(errornr, 1);
    }

    #[test]
    fn qf_get_nth_below_entry_counts_individual_entries() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = GotIntGuard::set(false);
        let entries = vec![entry_fl(7, 2), entry_fl(7, 2), entry_fl(7, 3)];
        let mut errornr = 1;

        unsafe { qf_get_nth_below_entry(&entries, 0, 2, false, &mut errornr) };

        assert_eq!(errornr, 3);
    }

    #[test]
    fn qf_get_nth_below_entry_counts_each_line_once() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = GotIntGuard::set(false);
        let entries = vec![
            entry_fl(7, 2),
            entry_fl(7, 2),
            entry_fl(7, 3),
            entry_fl(7, 3),
            entry_fl(7, 4),
        ];
        let mut errornr = 1;

        unsafe { qf_get_nth_below_entry(&entries, 0, 2, true, &mut errornr) };

        assert_eq!(errornr, 5, "two linewise steps land on line 4");
    }

    #[test]
    fn qf_get_nth_below_entry_rolls_back_a_partial_final_line() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = GotIntGuard::set(false);
        let entries = vec![entry_fl(7, 2), entry_fl(7, 2)];
        let mut errornr = 1;

        unsafe { qf_get_nth_below_entry(&entries, 0, 1, true, &mut errornr) };

        assert_eq!(errornr, 1, "there is no line below, so the scan is rolled back");
    }

    #[test]
    fn qf_get_nth_below_entry_stops_at_another_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = GotIntGuard::set(false);
        let entries = vec![entry_fl(7, 2), entry_fl(8, 3)];
        let mut errornr = 4;

        unsafe { qf_get_nth_below_entry(&entries, 0, 1, false, &mut errornr) };

        assert_eq!(errornr, 4);
    }

    #[test]
    fn qf_get_nth_above_entry_counts_individual_entries() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = GotIntGuard::set(false);
        let entries = vec![entry_fl(7, 2), entry_fl(7, 2), entry_fl(7, 3)];
        let mut errornr = 3;

        unsafe { qf_get_nth_above_entry(&entries, 2, 2, false, &mut errornr) };

        assert_eq!(errornr, 1);
    }

    #[test]
    fn qf_get_nth_above_entry_counts_each_line_once() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = GotIntGuard::set(false);
        let entries = vec![
            entry_fl(7, 2),
            entry_fl(7, 2),
            entry_fl(7, 3),
            entry_fl(7, 3),
            entry_fl(7, 4),
        ];
        let mut errornr = 5;

        unsafe { qf_get_nth_above_entry(&entries, 4, 2, true, &mut errornr) };

        assert_eq!(errornr, 1, "two linewise steps land on line 2's first entry");
    }

    #[test]
    fn qf_get_nth_above_entry_stops_at_another_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = GotIntGuard::set(false);
        let entries = vec![entry_fl(8, 2), entry_fl(7, 3)];
        let mut errornr = 9;

        unsafe { qf_get_nth_above_entry(&entries, 1, 1, false, &mut errornr) };

        assert_eq!(errornr, 9);
    }

    #[test]
    fn qf_get_nth_above_entry_stops_when_interrupted() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = GotIntGuard::set(true);
        let entries = vec![entry_fl(7, 2), entry_fl(7, 3)];
        let mut errornr = 2;

        unsafe { qf_get_nth_above_entry(&entries, 1, 1, false, &mut errornr) };

        assert_eq!(errornr, 2);
    }

    #[test]
    fn qf_find_nth_adj_entry_returns_the_closest_for_one() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = GotIntGuard::set(false);
        let qfl = qfl_with_entries(vec![
            entry_fl(7, 2),
            entry_fl(7, 5),
            entry_fl(7, 8),
        ]);

        assert_eq!(
            unsafe {
                qf_find_nth_adj_entry(
                    &qfl,
                    7,
                    &pos_at(5, 0),
                    1,
                    crate::vim_defs::Direction::Forward,
                    true,
                )
            },
            3
        );
    }

    #[test]
    fn qf_find_nth_adj_entry_walks_the_remaining_distance() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = GotIntGuard::set(false);
        let qfl = qfl_with_entries(vec![
            entry_fl(7, 2),
            entry_fl(7, 4),
            entry_fl(7, 6),
            entry_fl(7, 8),
        ]);

        assert_eq!(
            unsafe {
                qf_find_nth_adj_entry(
                    &qfl,
                    7,
                    &pos_at(3, 0),
                    3,
                    crate::vim_defs::Direction::Forward,
                    true,
                )
            },
            4,
            "closest is line 4, then two more steps reach line 8"
        );
        assert_eq!(
            unsafe {
                qf_find_nth_adj_entry(
                    &qfl,
                    7,
                    &pos_at(9, 0),
                    2,
                    crate::vim_defs::Direction::Backward,
                    true,
                )
            },
            3,
            "closest is line 8, then one step reaches line 6"
        );
    }

    #[test]
    fn qf_find_nth_adj_entry_returns_zero_when_no_entry_qualifies() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = GotIntGuard::set(false);
        let qfl = qfl_with_entries(vec![entry_fl(1, 2)]);

        assert_eq!(
            unsafe {
                qf_find_nth_adj_entry(
                    &qfl,
                    7,
                    &pos_at(1, 0),
                    1,
                    crate::vim_defs::Direction::Forward,
                    false,
                )
            },
            0
        );
    }

    #[test]
    fn qf_find_nth_adj_entry_counts_a_repeated_line_once() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = GotIntGuard::set(false);
        let qfl = qfl_with_entries(vec![
            entry_fl(7, 2),
            entry_fl(7, 2),
            entry_fl(7, 3),
            entry_fl(7, 4),
        ]);

        assert_eq!(
            unsafe {
                qf_find_nth_adj_entry(
                    &qfl,
                    7,
                    &pos_at(1, 0),
                    2,
                    crate::vim_defs::Direction::Forward,
                    true,
                )
            },
            3,
            "line 2 counts once, so the second result is line 3"
        );
    }

    #[test]
    fn qf_find_entry_before_pos_returns_none_when_the_first_is_not_before() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = GotIntGuard::set(false);
        let entries = vec![entry_fl(7, 5), entry_fl(7, 6)];
        let mut errornr = 4;

        assert_eq!(
            unsafe {
                qf_find_entry_before_pos(
                    &entries,
                    7,
                    &pos_at(5, 0),
                    false,
                    0,
                    &mut errornr,
                )
            },
            None
        );
        assert_eq!(errornr, 4);
    }

    #[test]
    fn qf_find_entry_before_pos_returns_the_last_strictly_before_entry() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = GotIntGuard::set(false);
        let mut entries = vec![
            entry_fl(7, 3),
            entry_fl(7, 5),
            entry_fl(7, 5),
            entry_fl(7, 6),
        ];
        entries[1].qf_col = 2;
        entries[2].qf_col = 4;
        let mut errornr = 10;

        assert_eq!(
            unsafe {
                qf_find_entry_before_pos(
                    &entries,
                    7,
                    &pos_at(5, 3),
                    false,
                    0,
                    &mut errornr,
                )
            },
            Some(1),
            "column 2 is the last entry strictly before column 3"
        );
        assert_eq!(errornr, 11);
    }

    #[test]
    fn qf_find_entry_before_pos_rewinds_to_the_first_entry_on_a_line() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = GotIntGuard::set(false);
        let entries = vec![
            entry_fl(7, 3),
            entry_fl(7, 5),
            entry_fl(7, 5),
            entry_fl(7, 6),
        ];
        let mut errornr = 1;

        assert_eq!(
            unsafe {
                qf_find_entry_before_pos(
                    &entries,
                    7,
                    &pos_at(6, 0),
                    true,
                    0,
                    &mut errornr,
                )
            },
            Some(1),
            "linewise returns the first entry on line 5"
        );
        assert_eq!(errornr, 2);
    }

    #[test]
    fn qf_find_entry_before_pos_stops_at_the_end_of_the_buffer_run() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = GotIntGuard::set(false);
        let entries = vec![entry_fl(7, 3), entry_fl(7, 4), entry_fl(8, 1)];
        let mut errornr = 6;

        assert_eq!(
            unsafe {
                qf_find_entry_before_pos(
                    &entries,
                    7,
                    &pos_at(9, 0),
                    false,
                    0,
                    &mut errornr,
                )
            },
            Some(1)
        );
        assert_eq!(errornr, 7);
    }

    #[test]
    fn qf_find_entry_after_pos_returns_the_start_when_it_is_already_after() {
        let entries = vec![entry_fl(7, 6), entry_fl(7, 9)];
        let mut errornr = 4;

        assert_eq!(
            qf_find_entry_after_pos(
                &entries,
                7,
                &pos_at(5, 99),
                false,
                0,
                &mut errornr,
            ),
            Some(0)
        );
        assert_eq!(errornr, 4, "the starting entry number is unchanged");
    }

    #[test]
    fn qf_find_entry_after_pos_skips_entries_on_or_before_the_position() {
        let mut entries = vec![
            entry_fl(7, 3),
            entry_fl(7, 5),
            entry_fl(7, 5),
            entry_fl(7, 6),
        ];
        entries[1].qf_col = 2;
        entries[2].qf_col = 4;
        let mut errornr = 10;

        assert_eq!(
            qf_find_entry_after_pos(
                &entries,
                7,
                &pos_at(5, 3),
                false,
                0,
                &mut errornr,
            ),
            Some(2),
            "the column-4 entry is the first strictly after column 3"
        );
        assert_eq!(errornr, 12);
    }

    #[test]
    fn qf_find_entry_after_pos_returns_none_at_the_end_of_the_buffer_run() {
        let entries = vec![entry_fl(7, 3), entry_fl(7, 5), entry_fl(8, 9)];
        let mut errornr = 2;

        assert_eq!(
            qf_find_entry_after_pos(
                &entries,
                7,
                &pos_at(9, 0),
                false,
                0,
                &mut errornr,
            ),
            None
        );
        assert_eq!(errornr, 3, "the scan advanced to the last entry in buffer 7");
    }

    #[test]
    fn qf_find_entry_after_pos_treats_a_line_as_one_when_linewise() {
        let mut entries = vec![entry_fl(7, 5), entry_fl(7, 5), entry_fl(7, 6)];
        entries[0].qf_col = 1;
        entries[1].qf_col = 99;
        let mut errornr = 1;

        assert_eq!(
            qf_find_entry_after_pos(
                &entries,
                7,
                &pos_at(5, 3),
                true,
                0,
                &mut errornr,
            ),
            Some(2),
            "both entries on line 5 are skipped regardless of column"
        );
        assert_eq!(errornr, 3);
    }

    /// At exactly the given position the four disagree, which is the
    /// whole reason all four exist.
    #[test]
    fn qf_entry_pos_helpers_differ_at_the_exact_position() {
        let e = entry_at(5, 3);
        let p = pos_at(5, 3);

        assert!(!qf_entry_after_pos(&e, &p, false), "not strictly after");
        assert!(!qf_entry_before_pos(&e, &p, false), "not strictly before");
        assert!(qf_entry_on_or_after_pos(&e, &p, false));
        assert!(qf_entry_on_or_before_pos(&e, &p, false));
    }

    #[test]
    fn qf_entry_pos_helpers_compare_columns_on_the_same_line() {
        let p = pos_at(5, 3);

        let later = entry_at(5, 4);
        assert!(qf_entry_after_pos(&later, &p, false));
        assert!(!qf_entry_before_pos(&later, &p, false));

        let earlier = entry_at(5, 2);
        assert!(!qf_entry_after_pos(&earlier, &p, false));
        assert!(qf_entry_before_pos(&earlier, &p, false));
    }

    /// Linewise ignores the column entirely, so an entry on the same
    /// line is neither after nor before regardless of its column.
    #[test]
    fn qf_entry_pos_helpers_ignore_the_column_when_linewise() {
        let p = pos_at(5, 3);
        for col in [0, 3, 99] {
            let e = entry_at(5, col);
            assert!(!qf_entry_after_pos(&e, &p, true), "col {col}");
            assert!(!qf_entry_before_pos(&e, &p, true), "col {col}");
            assert!(qf_entry_on_or_after_pos(&e, &p, true), "col {col}");
            assert!(qf_entry_on_or_before_pos(&e, &p, true), "col {col}");
        }
    }

    /// The line test stays strict in the "on or" variants: only the
    /// column comparison is relaxed. An entry on an earlier line must
    /// not count as "on or after" just because its column is large.
    #[test]
    fn qf_entry_on_or_after_keeps_the_line_test_strict() {
        let p = pos_at(5, 3);

        let earlier_line_big_col = entry_at(4, 99);
        assert!(!qf_entry_on_or_after_pos(&earlier_line_big_col, &p, false));

        let later_line_small_col = entry_at(6, 0);
        assert!(!qf_entry_on_or_before_pos(&later_line_small_col, &p, false));
        assert!(qf_entry_on_or_after_pos(&later_line_small_col, &p, false));
    }

    #[test]
    fn qf_entry_pos_helpers_compare_lines_first() {
        let p = pos_at(5, 3);

        let below = entry_at(9, 0);
        assert!(qf_entry_after_pos(&below, &p, false));
        assert!(qf_entry_after_pos(&below, &p, true));

        let above = entry_at(1, 99);
        assert!(qf_entry_before_pos(&above, &p, false));
        assert!(qf_entry_before_pos(&above, &p, true));
    }

    // --- qf_find_win_with_normal_buf ---

    /// A window list installed as `GLOBALS.firstwin`, owning every
    /// allocation as a raw pointer so writes through the walked
    /// pointers cannot invalidate a live `Box` tag.
    struct WinListFixture {
        wins: Vec<*mut WinT>,
        bufs: Vec<*mut BufT>,
        prev_firstwin: *mut WinT,
    }

    impl WinListFixture {
        /// Builds one window per entry, linked through `w_next`.
        /// `true` gives that window a normal buffer, `false` a
        /// non-normal one (`'buftype'` set).
        fn new(normal: &[bool]) -> Self {
            let mut wins = Vec::new();
            let mut bufs = Vec::new();
            for &is_normal in normal {
                let mut buf = Box::new(BufT::default());
                buf.b_p_bt = if is_normal {
                    Some(Vec::new())
                } else {
                    Some(b"quickfix".to_vec())
                };
                let buf = Box::into_raw(buf);
                bufs.push(buf);

                let mut win = Box::new(WinT::default());
                win.w_buffer = buf;
                wins.push(Box::into_raw(win));
            }
            for i in 0..wins.len().saturating_sub(1) {
                unsafe { &mut *wins[i] }.w_next = wins[i + 1];
            }

            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            let prev_firstwin = g.firstwin;
            g.firstwin = wins.first().copied().unwrap_or(std::ptr::null_mut());
            Self { wins, bufs, prev_firstwin }
        }
    }

    impl Drop for WinListFixture {
        fn drop(&mut self) {
            unsafe { crate::globals::GLOBALS.get_mut() }.firstwin = self.prev_firstwin;
            for &w in &self.wins {
                unsafe { drop(Box::from_raw(w)) };
            }
            for &b in &self.bufs {
                unsafe { drop(Box::from_raw(b)) };
            }
        }
    }

    #[test]
    fn qf_find_win_with_normal_buf_returns_null_for_no_windows() {
        let _lock = crate::globals::global_state_test_lock();
        let _fx = WinListFixture::new(&[]);
        assert!(unsafe { qf_find_win_with_normal_buf() }.is_null());
    }

    #[test]
    fn qf_find_win_with_normal_buf_returns_null_when_none_are_normal() {
        let _lock = crate::globals::global_state_test_lock();
        let _fx = WinListFixture::new(&[false, false]);
        assert!(unsafe { qf_find_win_with_normal_buf() }.is_null());
    }

    /// Returns the FIRST normal window, not merely any of them, so a
    /// reversed or last-wins walk would fail.
    #[test]
    fn qf_find_win_with_normal_buf_returns_the_first_normal_window() {
        let _lock = crate::globals::global_state_test_lock();
        let fx = WinListFixture::new(&[false, true, true]);
        let found = unsafe { qf_find_win_with_normal_buf() };
        assert_eq!(found, fx.wins[1], "the first normal window, skipping [0]");
    }

    #[test]
    fn qf_find_win_with_normal_buf_finds_a_normal_window_at_the_end() {
        let _lock = crate::globals::global_state_test_lock();
        let fx = WinListFixture::new(&[false, false, true]);
        assert_eq!(unsafe { qf_find_win_with_normal_buf() }, fx.wins[2]);
    }

    /// A window with no buffer at all is skipped rather than
    /// dereferenced.
    #[test]
    fn qf_find_win_with_normal_buf_skips_a_window_without_a_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let fx = WinListFixture::new(&[false, true]);
        let first = fx.wins[0];
        unsafe { &mut *first }.w_buffer = std::ptr::null_mut();
        assert_eq!(unsafe { qf_find_win_with_normal_buf() }, fx.wins[1]);
    }

    // --- copy_nonerror_line ---

    #[test]
    fn copy_nonerror_line_copies_the_line_and_nul_terminates() {
        let mut fields = QffieldsT::default();
        assert_eq!(
            copy_nonerror_line(b"some message", 12, &mut fields),
            qf_status::QF_OK
        );
        assert_eq!(fields.errmsg, b"some message\0".to_vec());
    }

    /// Only the first `linelen` bytes are taken, so trailing text in
    /// the buffer is not part of the message.
    #[test]
    fn copy_nonerror_line_uses_only_the_first_linelen_bytes() {
        let mut fields = QffieldsT::default();
        copy_nonerror_line(b"head and tail", 4, &mut fields);
        assert_eq!(fields.errmsg, b"head\0".to_vec());
    }

    /// The copy is NUL-scanned, not verbatim: `xstrlcpy` stops at the
    /// source's first NUL, so an embedded NUL truncates the message
    /// even when `linelen` reaches past it.
    #[test]
    fn copy_nonerror_line_truncates_at_an_embedded_nul() {
        let mut fields = QffieldsT::default();
        copy_nonerror_line(b"abc\0def", 7, &mut fields);
        assert_eq!(fields.errmsg, b"abc\0".to_vec());
    }

    /// A previous, longer message is fully replaced rather than
    /// partly overwritten.
    #[test]
    fn copy_nonerror_line_replaces_any_previous_message() {
        let mut fields = QffieldsT::default();
        copy_nonerror_line(b"a long previous message", 23, &mut fields);
        copy_nonerror_line(b"short", 5, &mut fields);
        assert_eq!(fields.errmsg, b"short\0".to_vec());
    }

    #[test]
    fn copy_nonerror_line_handles_an_empty_line() {
        let mut fields = QffieldsT::default();
        copy_nonerror_line(b"", 0, &mut fields);
        assert_eq!(fields.errmsg, b"\0".to_vec());
    }

    /// Only `errmsg` is touched; the other fields keep their values.
    #[test]
    fn copy_nonerror_line_leaves_the_other_fields_alone() {
        let mut fields = QffieldsT { lnum: 12, col: 3, valid: true, ..Default::default() };
        copy_nonerror_line(b"msg", 3, &mut fields);
        assert_eq!((fields.lnum, fields.col, fields.valid), (12, 3, true));
        assert!(fields.namebuf.is_empty());
    }

    #[test]
    fn qf_status_values_match_the_original() {
        assert_eq!(qf_status::QF_FAIL, 0);
        assert_eq!(qf_status::QF_OK, 1);
        assert_eq!(qf_status::QF_END_OF_INPUT, 2);
        assert_eq!(qf_status::QF_NOMEM, 3);
        assert_eq!(qf_status::QF_IGNORE_LINE, 4);
        assert_eq!(qf_status::QF_MULTISCAN, 5);
    }

    // --- qf_sync_llw_to_win ---

    /// A location list plus a window chain, all owned as raw pointers
    /// so that writes through the walked pointers cannot invalidate a
    /// live `Box`'s tag.
    struct LlwFixture {
        list: *mut crate::types_defs::QfInfoT,
        wins: Vec<*mut WinT>,
        bufs: Vec<*mut BufT>,
        prev_firstwin: *mut WinT,
    }

    impl LlwFixture {
        /// One window per `(is_quickfix, llist_matches)` entry, linked
        /// through `w_next` and installed as `firstwin`.
        fn new(spec: &[(bool, bool)]) -> Self {
            let list = Box::into_raw(Box::new(crate::types_defs::QfInfoT::default()));
            let mut wins = Vec::new();
            let mut bufs = Vec::new();

            for &(is_quickfix, llist_matches) in spec {
                let mut buf = Box::new(BufT::default());
                buf.b_p_bt = if is_quickfix {
                    Some(b"quickfix".to_vec())
                } else {
                    Some(Vec::new())
                };
                let buf = Box::into_raw(buf);
                bufs.push(buf);

                let win = Box::new(WinT {
                    w_buffer: buf,
                    w_llist: if llist_matches { list } else { std::ptr::null_mut() },
                    ..Default::default()
                });
                wins.push(Box::into_raw(win));
            }
            for i in 0..wins.len().saturating_sub(1) {
                unsafe { &mut *wins[i] }.w_next = wins[i + 1];
            }

            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            let prev_firstwin = g.firstwin;
            g.firstwin = wins.first().copied().unwrap_or(std::ptr::null_mut());
            Self { list, wins, bufs, prev_firstwin }
        }

        /// A standalone location-list window, NOT part of the chain,
        /// referring to this fixture's list via `w_llist_ref`.
        fn make_llw(&self, lhi: i64) -> *mut WinT {
            let mut win = Box::new(WinT { w_llist_ref: self.list, ..Default::default() });
            win.w_onebuf_opt.wo_lhi = lhi;
            Box::into_raw(win)
        }

        fn lhi(win: *mut WinT) -> i64 {
            unsafe { &*win }.w_onebuf_opt.wo_lhi
        }
    }

    impl Drop for LlwFixture {
        fn drop(&mut self) {
            unsafe { crate::globals::GLOBALS.get_mut() }.firstwin = self.prev_firstwin;
            unsafe {
                for &w in &self.wins {
                    drop(Box::from_raw(w));
                }
                for &b in &self.bufs {
                    drop(Box::from_raw(b));
                }
                drop(Box::from_raw(self.list));
            }
        }
    }

    #[test]
    fn qf_sync_llw_to_win_copies_lhistory_to_the_owning_window() {
        let _lock = crate::globals::global_state_test_lock();
        // One ordinary window owning the list.
        let fx = LlwFixture::new(&[(false, true)]);
        let llw = fx.make_llw(42);

        unsafe { qf_sync_llw_to_win(llw) };

        assert_eq!(LlwFixture::lhi(fx.wins[0]), 42);
        unsafe { drop(Box::from_raw(llw)) };
    }

    /// No window refers to the list, so nothing is written.
    #[test]
    fn qf_sync_llw_to_win_is_a_noop_when_no_window_owns_the_list() {
        let _lock = crate::globals::global_state_test_lock();
        let fx = LlwFixture::new(&[(false, false)]);
        let llw = fx.make_llw(42);

        unsafe { qf_sync_llw_to_win(llw) };

        assert_eq!(LlwFixture::lhi(fx.wins[0]), 0, "untouched");
        unsafe { drop(Box::from_raw(llw)) };
    }

    /// A quickfix window referring to the same list is NOT the target:
    /// the value belongs on the window owning the file. Here the
    /// quickfix window comes first in the chain, so a search that
    /// failed to skip it would write to the wrong window.
    #[test]
    fn qf_sync_llw_to_win_skips_a_quickfix_window() {
        let _lock = crate::globals::global_state_test_lock();
        let fx = LlwFixture::new(&[(true, true), (false, true)]);
        let llw = fx.make_llw(42);

        unsafe { qf_sync_llw_to_win(llw) };

        assert_eq!(LlwFixture::lhi(fx.wins[0]), 0, "quickfix window skipped");
        assert_eq!(LlwFixture::lhi(fx.wins[1]), 42, "ordinary window written");
        unsafe { drop(Box::from_raw(llw)) };
    }

    /// The original passes `w_llist_ref` through with no null check,
    /// so a window with no list reference matches the first ordinary
    /// window that also has none. Pinned to record that no guard was
    /// invented here.
    #[test]
    fn qf_sync_llw_to_win_passes_a_null_list_reference_through_unguarded() {
        let _lock = crate::globals::global_state_test_lock();
        // The window's w_llist is null, matching a null w_llist_ref.
        let fx = LlwFixture::new(&[(false, false)]);
        let mut llw = Box::new(WinT::default());
        llw.w_onebuf_opt.wo_lhi = 7;
        let llw = Box::into_raw(llw);

        unsafe { qf_sync_llw_to_win(llw) };

        assert_eq!(LlwFixture::lhi(fx.wins[0]), 7);
        unsafe { drop(Box::from_raw(llw)) };
    }

    // --- qf_sync_win_to_llw ---

    #[test]
    fn qf_sync_win_to_llw_copies_lhistory_to_the_matching_window() {
        let _lock = crate::globals::global_state_test_lock();
        let mut list = Box::new(crate::types_defs::QfInfoT::default());
        let list_ptr = std::ptr::addr_of_mut!(*list);

        // A quickfix-type window referring to the same location list.
        let mut qf_buf = Box::new(BufT::default());
        qf_buf.b_p_bt = Some(b"quickfix".to_vec());
        let qf_buf_ptr = std::ptr::addr_of_mut!(*qf_buf);
        let mut llw = Box::new(WinT {
            w_buffer: qf_buf_ptr,
            w_llist_ref: list_ptr,
            ..Default::default()
        });
        llw.w_onebuf_opt.wo_lhi = 1;
        let llw_ptr = std::ptr::addr_of_mut!(*llw);

        // The window whose 'lhistory' is being propagated.
        let mut buf = Box::new(BufT::default());
        let buf_ptr = std::ptr::addr_of_mut!(*buf);
        let mut pwp = Box::new(WinT {
            w_buffer: buf_ptr,
            w_llist: list_ptr,
            w_next: llw_ptr,
            ..Default::default()
        });
        pwp.w_onebuf_opt.wo_lhi = 42;
        let pwp_ptr = std::ptr::addr_of_mut!(*pwp);

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev = g.firstwin;
        g.firstwin = pwp_ptr;

        unsafe { qf_sync_win_to_llw(pwp_ptr) };

        let got = llw.w_onebuf_opt.wo_lhi;
        unsafe { crate::globals::GLOBALS.get_mut() }.firstwin = prev;
        assert_eq!(got, 42);
    }

    #[test]
    fn qf_sync_win_to_llw_is_a_noop_without_a_location_list() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = Box::new(BufT::default());
        let buf_ptr = std::ptr::addr_of_mut!(*buf);
        let mut pwp = Box::new(WinT { w_buffer: buf_ptr, ..Default::default() });
        pwp.w_onebuf_opt.wo_lhi = 42;
        let pwp_ptr = std::ptr::addr_of_mut!(*pwp);

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev = g.firstwin;
        g.firstwin = pwp_ptr;

        unsafe { qf_sync_win_to_llw(pwp_ptr) };

        unsafe { crate::globals::GLOBALS.get_mut() }.firstwin = prev;
        assert_eq!(pwp.w_onebuf_opt.wo_lhi, 42, "its own value is untouched");
    }

    #[test]
    fn qf_sync_win_to_llw_skips_a_window_referring_to_another_list() {
        // A location-list window for a DIFFERENT list must not be
        // updated, so the reference comparison genuinely matters.
        let _lock = crate::globals::global_state_test_lock();
        let mut list = Box::new(crate::types_defs::QfInfoT::default());
        let mut other = Box::new(crate::types_defs::QfInfoT::default());
        let list_ptr = std::ptr::addr_of_mut!(*list);
        let other_ptr = std::ptr::addr_of_mut!(*other);

        let mut qf_buf = Box::new(BufT::default());
        qf_buf.b_p_bt = Some(b"quickfix".to_vec());
        let qf_buf_ptr = std::ptr::addr_of_mut!(*qf_buf);
        let mut llw = Box::new(WinT {
            w_buffer: qf_buf_ptr,
            w_llist_ref: other_ptr,
            ..Default::default()
        });
        llw.w_onebuf_opt.wo_lhi = 1;
        let llw_ptr = std::ptr::addr_of_mut!(*llw);

        let mut buf = Box::new(BufT::default());
        let buf_ptr = std::ptr::addr_of_mut!(*buf);
        let mut pwp = Box::new(WinT {
            w_buffer: buf_ptr,
            w_llist: list_ptr,
            w_next: llw_ptr,
            ..Default::default()
        });
        pwp.w_onebuf_opt.wo_lhi = 42;
        let pwp_ptr = std::ptr::addr_of_mut!(*pwp);

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev = g.firstwin;
        g.firstwin = pwp_ptr;

        unsafe { qf_sync_win_to_llw(pwp_ptr) };

        let got = llw.w_onebuf_opt.wo_lhi;
        unsafe { crate::globals::GLOBALS.get_mut() }.firstwin = prev;
        assert_eq!(got, 1, "another list's window is left alone");
    }

    #[test]
    fn qf_sync_win_to_llw_skips_a_non_quickfix_window() {
        // Matching the list is not enough; the window must actually be
        // a quickfix-type one.
        let _lock = crate::globals::global_state_test_lock();
        let mut list = Box::new(crate::types_defs::QfInfoT::default());
        let list_ptr = std::ptr::addr_of_mut!(*list);

        let mut plain_buf = Box::new(BufT::default());
        let plain_buf_ptr = std::ptr::addr_of_mut!(*plain_buf);
        let mut other = Box::new(WinT {
            w_buffer: plain_buf_ptr,
            w_llist_ref: list_ptr,
            ..Default::default()
        });
        other.w_onebuf_opt.wo_lhi = 1;
        let other_ptr = std::ptr::addr_of_mut!(*other);

        let mut buf = Box::new(BufT::default());
        let buf_ptr = std::ptr::addr_of_mut!(*buf);
        let mut pwp = Box::new(WinT {
            w_buffer: buf_ptr,
            w_llist: list_ptr,
            w_next: other_ptr,
            ..Default::default()
        });
        pwp.w_onebuf_opt.wo_lhi = 42;
        let pwp_ptr = std::ptr::addr_of_mut!(*pwp);

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev = g.firstwin;
        g.firstwin = pwp_ptr;

        unsafe { qf_sync_win_to_llw(pwp_ptr) };

        let got = other.w_onebuf_opt.wo_lhi;
        unsafe { crate::globals::GLOBALS.get_mut() }.firstwin = prev;
        assert_eq!(got, 1, "a non-quickfix window is left alone");
    }

    // --- qf_find_help_win ---

    /// Boxed: these pointers are installed into GLOBALS, so they need
    /// stable heap addresses.
    fn help_win_fixture(
        specs: &[(bool, bool, bool)],
    ) -> (Vec<Box<BufT>>, Vec<Box<WinT>>) {
        let mut bufs: Vec<Box<BufT>> = Vec::new();
        let mut wins: Vec<Box<WinT>> = Vec::new();

        for &(is_help, hide, focusable) in specs {
            let mut buf = Box::new(BufT::default());
            buf.b_help = is_help;
            let buf_ptr = std::ptr::addr_of_mut!(*buf);
            let mut win = Box::new(WinT { w_buffer: buf_ptr, ..Default::default() });
            win.w_config.hide = hide;
            win.w_config.focusable = focusable;
            bufs.push(buf);
            wins.push(win);
        }

        // Chain them in order.
        for i in 0..wins.len() {
            let next = if i + 1 < wins.len() {
                std::ptr::addr_of_mut!(*wins[i + 1])
            } else {
                std::ptr::null_mut()
            };
            wins[i].w_next = next;
        }
        (bufs, wins)
    }

    fn with_windows<T>(wins: &mut [Box<WinT>], f: impl FnOnce() -> T) -> T {
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev = g.firstwin;
        g.firstwin = if wins.is_empty() {
            std::ptr::null_mut()
        } else {
            std::ptr::addr_of_mut!(*wins[0])
        };
        let r = f();
        unsafe { crate::globals::GLOBALS.get_mut() }.firstwin = prev;
        r
    }

    // --- qflist_valid ---

    #[test]
    fn qflist_valid_is_false_when_the_global_stack_is_uninitialized() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = QlInfoGuard::save();
        *unsafe { QL_INFO.get_mut() } = None;
        assert!(!unsafe { qflist_valid(std::ptr::null(), 1) });
    }

    #[test]
    fn qflist_valid_finds_a_live_list_on_the_global_stack() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = QlInfoGuard::save();

        let mut qi = qf_alloc_stack(QfltypeT::Quickfix, 3);
        qi.qf_lists[0].qf_id = 11;
        qi.qf_lists[1].qf_id = 22;
        qi.qf_listcount = 2;
        *unsafe { QL_INFO.get_mut() } = Some(qi);

        assert!(unsafe { qflist_valid(std::ptr::null(), 11) });
        assert!(unsafe { qflist_valid(std::ptr::null(), 22) });
        assert!(!unsafe { qflist_valid(std::ptr::null(), 33) });
    }

    /// Slots past `qf_listcount` are stale leftovers, not live lists.
    /// An implementation scanning the whole vector would wrongly
    /// report the id below as still valid.
    #[test]
    fn qflist_valid_ignores_slots_beyond_the_live_count() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = QlInfoGuard::save();

        let mut qi = qf_alloc_stack(QfltypeT::Quickfix, 3);
        qi.qf_lists[0].qf_id = 11;
        // A leftover id in a slot that is no longer live.
        qi.qf_lists[2].qf_id = 99;
        qi.qf_listcount = 1;
        *unsafe { QL_INFO.get_mut() } = Some(qi);

        assert!(unsafe { qflist_valid(std::ptr::null(), 11) });
        assert!(!unsafe { qflist_valid(std::ptr::null(), 99) });
    }

    /// With a window given, its OWN location list stack is consulted -
    /// not the global quickfix stack.
    #[test]
    fn qflist_valid_uses_the_windows_own_location_list() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = QlInfoGuard::save();

        // The global stack holds a different id, so a lookup that
        // fell back to it would give the wrong answer.
        let mut global = qf_alloc_stack(QfltypeT::Quickfix, 1);
        global.qf_lists[0].qf_id = 11;
        global.qf_listcount = 1;
        *unsafe { QL_INFO.get_mut() } = Some(global);

        let mut ll = Box::new(qf_alloc_stack(QfltypeT::Location, 1));
        ll.qf_lists[0].qf_id = 77;
        ll.qf_listcount = 1;

        let (_bufs, mut wins) =
            loclist_win_fixture(&[(std::ptr::addr_of_mut!(*ll), false)]);
        let wp = std::ptr::addr_of_mut!(*wins[0]);

        let (found77, found11) = with_windows(&mut wins, || unsafe {
            (qflist_valid(wp, 77), qflist_valid(wp, 11))
        });
        assert!(found77, "the window's own list must be found");
        assert!(!found11, "the global stack must not be consulted");
    }

    /// A window with no location list at all has nothing to validate.
    #[test]
    fn qflist_valid_is_false_for_a_window_without_a_location_list() {
        let _lock = crate::globals::global_state_test_lock();
        let (_bufs, mut wins) = loclist_win_fixture(&[(std::ptr::null_mut(), false)]);
        let wp = std::ptr::addr_of_mut!(*wins[0]);
        let got = with_windows(&mut wins, || unsafe { qflist_valid(wp, 1) });
        assert!(!got);
    }

    /// A window that no longer exists fails before its stack is even
    /// looked at - an autocmd may have closed it.
    #[test]
    fn qflist_valid_is_false_for_a_window_that_no_longer_exists() {
        let _lock = crate::globals::global_state_test_lock();
        let mut ll = Box::new(qf_alloc_stack(QfltypeT::Location, 1));
        ll.qf_lists[0].qf_id = 77;
        ll.qf_listcount = 1;

        let (_bufs, mut wins) =
            loclist_win_fixture(&[(std::ptr::addr_of_mut!(*ll), false)]);
        let wp = std::ptr::addr_of_mut!(*wins[0]);

        // The window is NOT installed in the window list, so
        // win_valid rejects it even though its stack holds the id.
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev = g.firstwin;
        g.firstwin = std::ptr::null_mut();
        let got = unsafe { qflist_valid(wp, 77) };
        unsafe { crate::globals::GLOBALS.get_mut() }.firstwin = prev;

        assert!(!got);
    }

    // --- ql_info / qf_init_stack / qf_stack_get_bufnr ---

    /// Restores `QL_INFO` on drop, even through a panic, so a failing
    /// test cannot leave a half-built global stack behind.
    struct QlInfoGuard(Option<crate::types_defs::QfInfoT>);

    impl QlInfoGuard {
        fn save() -> Self {
            Self(unsafe { QL_INFO.get_mut() }.take())
        }
    }

    impl Drop for QlInfoGuard {
        fn drop(&mut self) {
            *unsafe { QL_INFO.get_mut() } = self.0.take();
        }
    }

    /// The original's `ql_info` pointer starts NULL, so "not yet
    /// initialized" is a distinct state from "initialized but empty" -
    /// which is why reading the buffer number before initialization
    /// trips the original's own assert.
    #[test]
    #[should_panic(expected = "not initialized")]
    fn qf_stack_get_bufnr_panics_before_initialization() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = QlInfoGuard::save();
        *unsafe { QL_INFO.get_mut() } = None;
        let _ = unsafe { qf_stack_get_bufnr() };
    }

    #[test]
    fn qf_init_stack_builds_a_quickfix_stack_sized_by_chistory() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = QlInfoGuard::save();

        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let prev_chi = opts.p_chi;
        opts.p_chi = 7;

        unsafe { qf_init_stack() };

        let qi = unsafe { QL_INFO.get_mut() }.as_ref().expect("initialized");
        assert_eq!(qi.qf_maxcount, 7);
        assert_eq!(qi.qf_lists.len(), 7);
        assert_eq!(qi.qfl_type, QfltypeT::Quickfix);
        // An initialized stack still holds no lists yet - distinct
        // from being uninitialized.
        assert_eq!(qi.qf_listcount, 0);
        // The quickfix stack is a singleton in the original, so it is
        // NOT reference-counted like a location list.
        assert_eq!(qi.qf_refcount, 0);
        assert_eq!(qi.qf_bufnr, INVALID_QFBUFNR);

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_chi = prev_chi;
    }

    #[test]
    fn qf_stack_get_bufnr_reports_the_initialized_stacks_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = QlInfoGuard::save();

        let opts = unsafe { crate::option_vars::OPTION_VARS.get_mut() };
        let prev_chi = opts.p_chi;
        opts.p_chi = 2;
        unsafe { qf_init_stack() };

        assert_eq!(unsafe { qf_stack_get_bufnr() }, INVALID_QFBUFNR);

        // A real buffer number is reported once one is assigned.
        unsafe { QL_INFO.get_mut() }.as_mut().unwrap().qf_bufnr = 12;
        assert_eq!(unsafe { qf_stack_get_bufnr() }, 12);

        unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_chi = prev_chi;
    }

    #[test]
    fn qf_current_entry_uses_the_global_stack_for_an_ordinary_window() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = QlInfoGuard::save();
        let mut global = stack_with(1);
        global.qf_lists[0].qf_index = 4;
        *unsafe { QL_INFO.get_mut() } = Some(global);

        assert_eq!(unsafe { qf_current_entry(&WinT::default()) }, 4);
    }

    #[test]
    fn qf_current_entry_uses_a_location_list_windows_reference() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = QlInfoGuard::save();
        let mut global = stack_with(1);
        global.qf_lists[0].qf_index = 4;
        *unsafe { QL_INFO.get_mut() } = Some(global);

        let mut location = Box::new(stack_with(1));
        location.qfl_type = QfltypeT::Location;
        location.qf_lists[0].qf_index = 7;
        let mut buf = Box::new(BufT::default());
        buf.b_p_bt = Some(b"quickfix".to_vec());
        let win = WinT {
            w_buffer: std::ptr::addr_of_mut!(*buf),
            w_llist_ref: std::ptr::addr_of_mut!(*location),
            ..Default::default()
        };

        assert_eq!(unsafe { qf_current_entry(&win) }, 7);
    }

    #[test]
    fn qf_current_entry_ignores_a_reference_on_a_normal_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = QlInfoGuard::save();
        let mut global = stack_with(1);
        global.qf_lists[0].qf_index = 4;
        *unsafe { QL_INFO.get_mut() } = Some(global);

        let mut location = Box::new(stack_with(1));
        location.qf_lists[0].qf_index = 7;
        let mut buf = Box::new(BufT::default());
        let win = WinT {
            w_buffer: std::ptr::addr_of_mut!(*buf),
            w_llist_ref: std::ptr::addr_of_mut!(*location),
            ..Default::default()
        };

        assert_eq!(unsafe { qf_current_entry(&win) }, 4);
    }

    // --- win_set_loclist / qf_find_win_with_loclist ---

    /// Builds windows with the given `(w_llist, is_quickfix_buffer)`
    /// specs, boxed for stable addresses since they go into GLOBALS.
    fn loclist_win_fixture(
        specs: &[(*mut crate::types_defs::QfInfoT, bool)],
    ) -> (Vec<Box<BufT>>, Vec<Box<WinT>>) {
        let mut bufs: Vec<Box<BufT>> = Vec::new();
        let mut wins: Vec<Box<WinT>> = Vec::new();

        for &(llist, is_qf) in specs {
            let mut buf = Box::new(BufT::default());
            if is_qf {
                buf.b_p_bt = Some(b"quickfix".to_vec());
            }
            let buf_ptr = std::ptr::addr_of_mut!(*buf);
            let win = Box::new(WinT { w_buffer: buf_ptr, w_llist: llist, ..Default::default() });
            bufs.push(buf);
            wins.push(win);
        }

        for i in 0..wins.len() {
            let next = if i + 1 < wins.len() {
                std::ptr::addr_of_mut!(*wins[i + 1])
            } else {
                std::ptr::null_mut()
            };
            wins[i].w_next = next;
        }
        (bufs, wins)
    }

    #[test]
    fn win_set_loclist_attaches_the_stack_and_takes_a_reference() {
        let mut qi = Box::new(crate::types_defs::QfInfoT::default());
        let qi_ptr = std::ptr::addr_of_mut!(*qi);
        let mut win = Box::new(WinT::default());

        unsafe { win_set_loclist(std::ptr::addr_of_mut!(*win), qi_ptr) };

        assert_eq!(win.w_llist, qi_ptr);
        assert_eq!(qi.qf_refcount, 1, "the window now holds a reference");

        // A second window attaching bumps it again.
        let mut win2 = Box::new(WinT::default());
        unsafe { win_set_loclist(std::ptr::addr_of_mut!(*win2), qi_ptr) };
        assert_eq!(qi.qf_refcount, 2);
    }

    #[test]
    fn find_win_with_loclist_finds_the_matching_non_quickfix_window() {
        let _lock = crate::globals::global_state_test_lock();
        let mut qi = Box::new(crate::types_defs::QfInfoT::default());
        let qi_ptr = std::ptr::addr_of_mut!(*qi);

        // A non-matching window first, so a match on the SECOND is
        // required.
        let (_bufs, mut wins) =
            loclist_win_fixture(&[(std::ptr::null_mut(), false), (qi_ptr, false)]);
        let want = std::ptr::addr_of_mut!(*wins[1]);

        let found = with_windows(&mut wins, || unsafe { qf_find_win_with_loclist(qi_ptr) });
        assert_eq!(found, want);
    }

    /// A quickfix window using the very same list must be SKIPPED -
    /// this looks for the window whose file the list belongs to, not
    /// the window displaying the list. An implementation that only
    /// compared the list pointer would wrongly return the first one.
    #[test]
    fn find_win_with_loclist_skips_quickfix_windows() {
        let _lock = crate::globals::global_state_test_lock();
        let mut qi = Box::new(crate::types_defs::QfInfoT::default());
        let qi_ptr = std::ptr::addr_of_mut!(*qi);

        let (_bufs, mut wins) = loclist_win_fixture(&[(qi_ptr, true), (qi_ptr, false)]);
        let want = std::ptr::addr_of_mut!(*wins[1]);

        let found = with_windows(&mut wins, || unsafe { qf_find_win_with_loclist(qi_ptr) });
        assert_eq!(found, want, "the quickfix window must be skipped");
    }

    #[test]
    fn find_win_with_loclist_is_null_when_only_quickfix_windows_match() {
        let _lock = crate::globals::global_state_test_lock();
        let mut qi = Box::new(crate::types_defs::QfInfoT::default());
        let qi_ptr = std::ptr::addr_of_mut!(*qi);

        let (_bufs, mut wins) = loclist_win_fixture(&[(qi_ptr, true)]);
        let found = with_windows(&mut wins, || unsafe { qf_find_win_with_loclist(qi_ptr) });
        assert!(found.is_null());
    }

    #[test]
    fn find_win_with_loclist_is_null_for_a_different_stack() {
        let _lock = crate::globals::global_state_test_lock();
        let mut qi = Box::new(crate::types_defs::QfInfoT::default());
        let mut other = Box::new(crate::types_defs::QfInfoT::default());
        let qi_ptr = std::ptr::addr_of_mut!(*qi);
        let other_ptr = std::ptr::addr_of_mut!(*other);

        let (_bufs, mut wins) = loclist_win_fixture(&[(other_ptr, false)]);
        let found = with_windows(&mut wins, || unsafe { qf_find_win_with_loclist(qi_ptr) });
        assert!(found.is_null());
    }

    #[test]
    fn qf_find_help_win_finds_a_usable_help_window() {
        let _lock = crate::globals::global_state_test_lock();
        // Non-help first, so a match on the SECOND window is required.
        let (_bufs, mut wins) = help_win_fixture(&[(false, false, true), (true, false, true)]);
        let want = std::ptr::addr_of_mut!(*wins[1]);

        let found = with_windows(&mut wins, || unsafe { qf_find_help_win() });
        assert_eq!(found, want);
    }

    #[test]
    fn qf_find_help_win_skips_hidden_and_unfocusable_help_windows() {
        let _lock = crate::globals::global_state_test_lock();
        // A hidden help window and an unfocusable one cannot be jumped
        // into, so neither counts.
        let (_bufs, mut wins) = help_win_fixture(&[(true, true, true), (true, false, false)]);

        let found = with_windows(&mut wins, || unsafe { qf_find_help_win() });
        assert!(found.is_null());
    }

    #[test]
    fn qf_find_help_win_is_null_without_any_help_window() {
        let _lock = crate::globals::global_state_test_lock();
        let (_bufs, mut wins) = help_win_fixture(&[(false, false, true)]);

        let found = with_windows(&mut wins, || unsafe { qf_find_help_win() });
        assert!(found.is_null());
    }

    // --- qf_pop_dir / qf_clean_dir_stack ---

    fn dir_stack(dirs: &[&[u8]]) -> Option<Box<DirStackT>> {
        // Built top-first, so dirs[0] ends up on top of the stack.
        let mut head: Option<Box<DirStackT>> = None;
        for d in dirs.iter().rev() {
            head = Some(Box::new(DirStackT { next: head, dirname: Some(d.to_vec()) }));
        }
        head
    }

    #[test]
    fn qf_pop_dir_reports_the_directory_now_on_top() {
        let mut stack = dir_stack(&[b"/top", b"/middle", b"/bottom"]);

        assert_eq!(qf_pop_dir(&mut stack).as_deref(), Some(&b"/middle"[..]));
        assert_eq!(qf_pop_dir(&mut stack).as_deref(), Some(&b"/bottom"[..]));
    }

    #[test]
    fn qf_pop_dir_reports_nothing_once_the_stack_is_emptied() {
        let mut stack = dir_stack(&[b"/only"]);

        // Popping the single entry leaves nothing on top.
        assert_eq!(qf_pop_dir(&mut stack), None);
        assert!(stack.is_none());
    }

    #[test]
    fn qf_pop_dir_on_an_empty_stack_is_a_noop() {
        let mut stack: Option<Box<DirStackT>> = None;
        assert_eq!(qf_pop_dir(&mut stack), None);
        assert!(stack.is_none());
    }

    #[test]
    fn qf_clean_dir_stack_empties_the_whole_stack() {
        let mut stack = dir_stack(&[b"/a", b"/b", b"/c"]);
        qf_clean_dir_stack(&mut stack);
        assert!(stack.is_none());
    }

    #[test]
    fn qf_clean_dir_stack_on_an_empty_stack_is_a_noop() {
        let mut stack: Option<Box<DirStackT>> = None;
        qf_clean_dir_stack(&mut stack);
        assert!(stack.is_none());
    }

    #[test]
    fn qf_clean_dir_stack_handles_a_long_chain_without_overflowing() {
        // The iterative teardown exists so a deep stack cannot blow
        // the call stack via recursive Drop.
        let dirs: Vec<Vec<u8>> = (0..50_000).map(|i| format!("/d{i}").into_bytes()).collect();
        let refs: Vec<&[u8]> = dirs.iter().map(std::vec::Vec::as_slice).collect();
        let mut stack = dir_stack(&refs);

        qf_clean_dir_stack(&mut stack);
        assert!(stack.is_none());
    }

    // --- efm_option_part_len ---

    #[test]
    fn efm_option_part_len_stops_at_the_first_comma() {
        // Cross-verified against real nvim that 'errorformat' parts
        // are comma separated.
        assert_eq!(efm_option_part_len(b"%f:%l:%m,%-G%.%#"), 8);
        assert_eq!(efm_option_part_len(b"%f:%l:%m"), 8, "no comma: the whole string");
    }

    #[test]
    fn efm_option_part_len_treats_an_escaped_comma_as_content() {
        // A backslash escapes the next byte, so this comma does not
        // end the part - the whole 5 bytes belong to it.
        assert_eq!(efm_option_part_len(b"a\\,b"), 4);
        // The following unescaped comma still does.
        assert_eq!(efm_option_part_len(b"a\\,b,c"), 4);
    }

    #[test]
    fn efm_option_part_len_does_not_escape_past_the_end() {
        // Matches the original's own `efm[len + 1] != NUL` guard: a
        // trailing backslash consumes only itself.
        assert_eq!(efm_option_part_len(b"ab\\"), 3);
        assert_eq!(efm_option_part_len(b"\\"), 1);
    }

    #[test]
    fn efm_option_part_len_handles_empty_and_leading_comma() {
        assert_eq!(efm_option_part_len(b""), 0);
        assert_eq!(efm_option_part_len(b",rest"), 0);
        // A NUL terminator ends the scan just like the end of slice.
        assert_eq!(efm_option_part_len(b"ab\0cd"), 2);
    }

    /// A stack holding `count` freshly-defaulted lists, with
    /// `qf_listcount` kept consistent with them.
    fn stack_with(count: usize) -> crate::types_defs::QfInfoT {
        crate::types_defs::QfInfoT {
            qf_listcount: i32::try_from(count).unwrap(),
            qf_lists: (0..count).map(|_| QfListT::default()).collect(),
            ..Default::default()
        }
    }

    /// A list holding `count` entries, all valid.
    fn list_with(count: usize) -> QfListT {
        QfListT {
            qf_entries: (0..count).map(|_| QflineT::default()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn qf_stack_empty_treats_a_missing_stack_as_empty() {
        assert!(qf_stack_empty(None));
    }

    #[test]
    fn qf_stack_empty_follows_qf_listcount_not_the_vector() {
        assert!(qf_stack_empty(Some(&stack_with(0))));
        assert!(!qf_stack_empty(Some(&stack_with(1))));

        // The original tests qf_listcount alone, so a stack whose
        // count says zero reads as empty even holding a list.
        let mut qi = stack_with(1);
        qi.qf_listcount = 0;
        assert!(qf_stack_empty(Some(&qi)));
    }

    #[test]
    fn qf_list_empty_treats_a_missing_list_as_empty() {
        assert!(qf_list_empty(None));
    }

    #[test]
    fn qf_list_empty_follows_the_entry_count() {
        assert!(qf_list_empty(Some(&QfListT::default())));
        assert!(!qf_list_empty(Some(&list_with(1))));
    }

    #[test]
    fn qf_getprop_idx_uses_an_explicit_requested_index() {
        let qfl = QfListT {
            qf_index: 3,
            ..list_with(5)
        };
        assert_eq!(qf_getprop_idx(&qfl, 4), 4);
    }

    #[test]
    fn qf_getprop_idx_defaults_to_the_current_index() {
        let qfl = QfListT {
            qf_index: 3,
            ..list_with(5)
        };
        assert_eq!(qf_getprop_idx(&qfl, 0), 3);
    }

    #[test]
    fn qf_getprop_idx_reports_zero_for_an_empty_list() {
        let qfl = QfListT {
            qf_index: 9,
            ..Default::default()
        };
        assert_eq!(qf_getprop_idx(&qfl, 0), 0);
    }

    #[test]
    fn qf_getprop_title_adds_the_owned_title_string() {
        let _lock = crate::globals::global_state_test_lock();
        let mut dict = TestDict::new();
        let qfl = QfListT {
            qf_title: Some(b"build errors".to_vec()),
            ..Default::default()
        };

        assert_eq!(
            qf_getprop_title(&qfl, dict.get()),
            crate::vim_defs::OK
        );
        let item = crate::eval::typval::tv_dict_find(
            Some(dict.get()),
            b"title",
        )
        .unwrap();
        assert!(matches!(
            unsafe { &(*item).di_tv.value },
            crate::eval::typval_defs::TypvalValue::String(Some(title))
                if title == b"build errors"
        ));
    }

    #[test]
    fn qf_getprop_title_preserves_a_null_title() {
        let _lock = crate::globals::global_state_test_lock();
        let mut dict = TestDict::new();

        assert_eq!(
            qf_getprop_title(&QfListT::default(), dict.get()),
            crate::vim_defs::OK
        );
        let item = crate::eval::typval::tv_dict_find(
            Some(dict.get()),
            b"title",
        )
        .unwrap();
        assert!(matches!(
            unsafe { &(*item).di_tv.value },
            crate::eval::typval_defs::TypvalValue::String(None)
        ));
    }

    #[test]
    fn qf_getprop_filewinid_reports_zero_without_a_location_window() {
        let _lock = crate::globals::global_state_test_lock();
        let mut dict = TestDict::new();

        assert_eq!(
            unsafe {
                qf_getprop_filewinid(None, std::ptr::null(), dict.get())
            },
            crate::vim_defs::OK
        );
        let item = crate::eval::typval::tv_dict_find(
            Some(dict.get()),
            b"filewinid",
        )
        .unwrap();
        assert!(matches!(
            unsafe { &(*item).di_tv.value },
            crate::eval::typval_defs::TypvalValue::Number(0)
        ));
    }

    #[test]
    fn qf_getprop_filewinid_finds_the_location_lists_file_window() {
        let _lock = crate::globals::global_state_test_lock();
        let mut dict = TestDict::new();
        let mut qi = Box::new(crate::types_defs::QfInfoT {
            qfl_type: QfltypeT::Location,
            ..Default::default()
        });
        let qi_ptr = std::ptr::addr_of_mut!(*qi);

        let mut list_buf = Box::new(BufT::default());
        list_buf.b_p_bt = Some(b"quickfix".to_vec());
        let list_win = WinT {
            w_buffer: std::ptr::addr_of_mut!(*list_buf),
            w_llist_ref: qi_ptr,
            ..Default::default()
        };

        let mut file_buf = Box::new(BufT::default());
        let mut file_win = Box::new(WinT {
            handle: 456,
            w_buffer: std::ptr::addr_of_mut!(*file_buf),
            w_llist: qi_ptr,
            ..Default::default()
        });
        let file_win_ptr = std::ptr::addr_of_mut!(*file_win);
        let _firstwin = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.firstwin,
                file_win_ptr,
            )
        };

        assert_eq!(
            unsafe { qf_getprop_filewinid(Some(&list_win), qi_ptr, dict.get()) },
            crate::vim_defs::OK
        );
        let item = crate::eval::typval::tv_dict_find(
            Some(dict.get()),
            b"filewinid",
        )
        .unwrap();
        assert!(matches!(
            unsafe { &(*item).di_tv.value },
            crate::eval::typval_defs::TypvalValue::Number(456)
        ));
    }

    #[test]
    fn qf_count_is_the_number_of_entries() {
        assert_eq!(QfListT::default().qf_count(), 0);
        assert_eq!(list_with(3).qf_count(), 3);
    }

    #[test]
    fn qf_list_has_valid_entries_needs_both_conditions() {
        // Empty -> false regardless of qf_nonevalid.
        assert!(!qf_list_has_valid_entries(&QfListT::default()));

        // Non-empty and some entry valid -> true.
        let qfl = list_with(2);
        assert!(qf_list_has_valid_entries(&qfl));

        // Non-empty but nothing valid -> false.
        let qfl = QfListT { qf_nonevalid: true, ..list_with(2) };
        assert!(!qf_list_has_valid_entries(&qfl));
    }

    #[test]
    fn qf_get_list_returns_the_list_at_an_index() {
        let mut qi = stack_with(2);
        qi.qf_lists[1].qf_id = 77;
        assert_eq!(qf_get_list(&qi, 1).unwrap().qf_id, 77);
    }

    #[test]
    fn qf_get_list_rejects_out_of_range_indices() {
        let qi = stack_with(1);
        // The original would read past the array here; returning None
        // keeps that from panicking.
        assert!(qf_get_list(&qi, 1).is_none());
        assert!(qf_get_list(&qi, INVALID_QFIDX).is_none());
    }

    #[test]
    fn qf_get_curlist_follows_qf_curlist() {
        let mut qi = stack_with(3);
        qi.qf_lists[2].qf_id = 9;
        qi.qf_curlist = 2;
        assert_eq!(qf_get_curlist(&qi).unwrap().qf_id, 9);
    }

    #[test]
    fn stack_and_list_type_predicates_are_mutually_exclusive() {
        let qf = crate::types_defs::QfInfoT {
            qfl_type: QfltypeT::Quickfix,
            ..Default::default()
        };
        assert!(is_qf_stack(&qf) && !is_ll_stack(&qf));

        let ll = crate::types_defs::QfInfoT {
            qfl_type: QfltypeT::Location,
            ..Default::default()
        };
        assert!(is_ll_stack(&ll) && !is_qf_stack(&ll));

        // An internal list is neither.
        let int = crate::types_defs::QfInfoT {
            qfl_type: QfltypeT::Internal,
            ..Default::default()
        };
        assert!(!is_qf_stack(&int) && !is_ll_stack(&int));

        let qfl = QfListT { qfl_type: QfltypeT::Quickfix, ..Default::default() };
        assert!(is_qf_list(&qfl) && !is_ll_list(&qfl));
        let lll = QfListT { qfl_type: QfltypeT::Location, ..Default::default() };
        assert!(is_ll_list(&lll) && !is_qf_list(&lll));
    }

    #[test]
    fn is_ll_window_requires_a_quickfix_buffer_and_list_reference() {
        let mut buf = Box::new(BufT::default());
        buf.b_p_bt = Some(b"quickfix".to_vec());
        let mut qi = Box::new(crate::types_defs::QfInfoT::default());
        let win = WinT {
            w_buffer: std::ptr::addr_of_mut!(*buf),
            w_llist_ref: std::ptr::addr_of_mut!(*qi),
            ..Default::default()
        };

        assert!(is_ll_window(&win));
    }

    #[test]
    fn is_ll_window_rejects_a_quickfix_window_without_a_list_reference() {
        let mut buf = Box::new(BufT::default());
        buf.b_p_bt = Some(b"quickfix".to_vec());
        let win = WinT {
            w_buffer: std::ptr::addr_of_mut!(*buf),
            ..Default::default()
        };

        assert!(!is_ll_window(&win));
    }

    #[test]
    fn is_ll_window_rejects_a_normal_buffer_with_a_list_reference() {
        let mut buf = Box::new(BufT::default());
        let mut qi = Box::new(crate::types_defs::QfInfoT::default());
        let win = WinT {
            w_buffer: std::ptr::addr_of_mut!(*buf),
            w_llist_ref: std::ptr::addr_of_mut!(*qi),
            ..Default::default()
        };

        assert!(!is_ll_window(&win));
    }

    #[test]
    fn is_qf_win_accepts_the_global_quickfix_window_shape() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = Box::new(BufT::default());
        buf.b_p_bt = Some(b"quickfix".to_vec());
        let buf_ptr = std::ptr::addr_of_mut!(*buf);
        let _buf_guard = unsafe {
            crate::globals::GlobalFieldGuard::install(|g| &mut g.lastbuf, buf_ptr)
        };
        let qi = Box::new(crate::types_defs::QfInfoT::default());
        let win = WinT {
            w_buffer: buf_ptr,
            ..Default::default()
        };

        assert!(unsafe { is_qf_win(&win, std::ptr::addr_of!(*qi)) });
    }

    #[test]
    fn is_qf_win_accepts_only_the_matching_location_list() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = Box::new(BufT::default());
        buf.b_p_bt = Some(b"quickfix".to_vec());
        let buf_ptr = std::ptr::addr_of_mut!(*buf);
        let _buf_guard = unsafe {
            crate::globals::GlobalFieldGuard::install(|g| &mut g.lastbuf, buf_ptr)
        };
        let mut qi = Box::new(crate::types_defs::QfInfoT {
            qfl_type: QfltypeT::Location,
            ..Default::default()
        });
        let other = Box::new(crate::types_defs::QfInfoT {
            qfl_type: QfltypeT::Location,
            ..Default::default()
        });
        let win = WinT {
            w_buffer: buf_ptr,
            w_llist_ref: std::ptr::addr_of_mut!(*qi),
            ..Default::default()
        };

        assert!(unsafe { is_qf_win(&win, std::ptr::addr_of!(*qi)) });
        assert!(
            !unsafe { is_qf_win(&win, std::ptr::addr_of!(*other)) },
            "another location-list stack must not match"
        );
    }

    #[test]
    fn is_qf_win_rejects_an_invalid_or_non_quickfix_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let qi = Box::new(crate::types_defs::QfInfoT::default());
        let mut normal = Box::new(BufT::default());
        let normal_ptr = std::ptr::addr_of_mut!(*normal);
        let _buf_guard = unsafe {
            crate::globals::GlobalFieldGuard::install(|g| &mut g.lastbuf, normal_ptr)
        };

        let invalid = WinT::default();
        assert!(!unsafe { is_qf_win(&invalid, std::ptr::addr_of!(*qi)) });

        let normal_win = WinT {
            w_buffer: normal_ptr,
            ..Default::default()
        };
        assert!(!unsafe { is_qf_win(&normal_win, std::ptr::addr_of!(*qi)) });
    }

    #[test]
    fn qf_find_win_returns_the_matching_window_not_merely_the_first() {
        let _lock = crate::globals::global_state_test_lock();
        let mut qi = Box::new(crate::types_defs::QfInfoT {
            qfl_type: QfltypeT::Location,
            ..Default::default()
        });
        let mut other = Box::new(crate::types_defs::QfInfoT {
            qfl_type: QfltypeT::Location,
            ..Default::default()
        });
        let qi_ptr = std::ptr::addr_of_mut!(*qi);
        let other_ptr = std::ptr::addr_of_mut!(*other);

        let mut buf1 = Box::new(BufT::default());
        let mut buf2 = Box::new(BufT::default());
        buf1.b_p_bt = Some(b"quickfix".to_vec());
        buf2.b_p_bt = Some(b"quickfix".to_vec());
        let buf1_ptr = std::ptr::addr_of_mut!(*buf1);
        let buf2_ptr = std::ptr::addr_of_mut!(*buf2);
        buf2.b_prev = buf1_ptr;

        let mut win1 = Box::new(WinT {
            w_buffer: buf1_ptr,
            w_llist_ref: other_ptr,
            ..Default::default()
        });
        let mut win2 = Box::new(WinT {
            w_buffer: buf2_ptr,
            w_llist_ref: qi_ptr,
            ..Default::default()
        });
        let win2_ptr = std::ptr::addr_of_mut!(*win2);
        win1.w_next = win2_ptr;
        let win1_ptr = std::ptr::addr_of_mut!(*win1);

        let _firstwin = unsafe {
            crate::globals::GlobalFieldGuard::install(|g| &mut g.firstwin, win1_ptr)
        };
        let _lastbuf = unsafe {
            crate::globals::GlobalFieldGuard::install(|g| &mut g.lastbuf, buf2_ptr)
        };

        assert_eq!(unsafe { qf_find_win(qi_ptr) }, win2_ptr);
    }

    #[test]
    fn qf_find_win_returns_null_when_no_window_matches() {
        let _lock = crate::globals::global_state_test_lock();
        let mut qi = Box::new(crate::types_defs::QfInfoT {
            qfl_type: QfltypeT::Location,
            ..Default::default()
        });
        let mut other = Box::new(crate::types_defs::QfInfoT {
            qfl_type: QfltypeT::Location,
            ..Default::default()
        });
        let mut buf = Box::new(BufT::default());
        buf.b_p_bt = Some(b"quickfix".to_vec());
        let buf_ptr = std::ptr::addr_of_mut!(*buf);
        let mut win = Box::new(WinT {
            w_buffer: buf_ptr,
            w_llist_ref: std::ptr::addr_of_mut!(*other),
            ..Default::default()
        });
        let win_ptr = std::ptr::addr_of_mut!(*win);
        let _firstwin = unsafe {
            crate::globals::GlobalFieldGuard::install(|g| &mut g.firstwin, win_ptr)
        };
        let _lastbuf = unsafe {
            crate::globals::GlobalFieldGuard::install(|g| &mut g.lastbuf, buf_ptr)
        };

        assert!(unsafe { qf_find_win(std::ptr::addr_of_mut!(*qi)) }.is_null());
    }

    #[test]
    fn qf_find_win_handles_an_empty_window_chain() {
        let _lock = crate::globals::global_state_test_lock();
        let qi = Box::new(crate::types_defs::QfInfoT::default());
        let _firstwin = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.firstwin,
                std::ptr::null_mut(),
            )
        };

        assert!(unsafe { qf_find_win(std::ptr::addr_of!(*qi)) }.is_null());
    }

    #[test]
    fn qf_winid_is_zero_for_a_null_stack() {
        assert_eq!(unsafe { qf_winid(std::ptr::null()) }, 0);
    }

    #[test]
    fn qf_winid_returns_the_matching_windows_handle() {
        let _lock = crate::globals::global_state_test_lock();
        let qi = Box::new(crate::types_defs::QfInfoT::default());
        let mut buf = Box::new(BufT::default());
        buf.b_p_bt = Some(b"quickfix".to_vec());
        let buf_ptr = std::ptr::addr_of_mut!(*buf);
        let mut win = Box::new(WinT {
            handle: 314,
            w_buffer: buf_ptr,
            ..Default::default()
        });
        let win_ptr = std::ptr::addr_of_mut!(*win);
        let _firstwin = unsafe {
            crate::globals::GlobalFieldGuard::install(|g| &mut g.firstwin, win_ptr)
        };
        let _lastbuf = unsafe {
            crate::globals::GlobalFieldGuard::install(|g| &mut g.lastbuf, buf_ptr)
        };

        assert_eq!(unsafe { qf_winid(std::ptr::addr_of!(*qi)) }, 314);
    }

    #[test]
    fn qf_getprop_qfbufnr_is_zero_without_a_stack() {
        assert_eq!(unsafe { qf_getprop_qfbufnr(None) }, 0);
    }

    #[test]
    fn qf_getprop_qfbufnr_returns_a_live_quickfix_buffer_number() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = Box::new(BufT {
            handle: 12,
            ..Default::default()
        });
        let buf_ptr = std::ptr::addr_of_mut!(*buf);
        let _lastbuf = unsafe {
            crate::globals::GlobalFieldGuard::install(|g| &mut g.lastbuf, buf_ptr)
        };
        let qi = crate::types_defs::QfInfoT {
            qf_bufnr: 12,
            ..Default::default()
        };

        assert_eq!(unsafe { qf_getprop_qfbufnr(Some(&qi)) }, 12);
    }

    #[test]
    fn qf_getprop_qfbufnr_is_zero_after_the_buffer_is_wiped() {
        let _lock = crate::globals::global_state_test_lock();
        let _lastbuf = unsafe {
            crate::globals::GlobalFieldGuard::install(
                |g| &mut g.lastbuf,
                std::ptr::null_mut(),
            )
        };
        let qi = crate::types_defs::QfInfoT {
            qf_bufnr: 12,
            ..Default::default()
        };

        assert_eq!(unsafe { qf_getprop_qfbufnr(Some(&qi)) }, 0);
    }

    #[test]
    fn a_default_stack_is_a_quickfix_stack() {
        // QFLT_QUICKFIX is the original enum's own first (zero) value.
        assert_eq!(QfltypeT::default(), QfltypeT::Quickfix);
    }

    /// Resets `LAST_QF_ID` after a test so the shared id counter
    /// cannot leak into another test's expectations.
    struct LastQfIdGuard {
        saved: u32,
    }

    impl LastQfIdGuard {
        fn new() -> Self {
            Self { saved: *unsafe { LAST_QF_ID.get_mut() } }
        }
    }

    impl Drop for LastQfIdGuard {
        fn drop(&mut self) {
            *unsafe { LAST_QF_ID.get_mut() } = self.saved;
        }
    }

    /// A list of `n` entries, all valid, each in its own file.
    fn nav_list(n: usize) -> QfListT {
        QfListT {
            qf_entries: (0..n)
                .map(|i| QflineT {
                    qf_valid: true,
                    qf_fnum: i32::try_from(i).unwrap() + 1,
                    ..Default::default()
                })
                .collect(),
            qf_index: 1,
            ..Default::default()
        }
    }

    #[test]
    fn entry_at_uses_one_based_indexing() {
        let qfl = nav_list(3);
        assert_eq!(qfl.entry_at(1).unwrap().qf_fnum, 1);
        assert_eq!(qfl.entry_at(3).unwrap().qf_fnum, 3);
        // Quickfix entry numbers start at 1, so 0 is out of range.
        assert!(qfl.entry_at(0).is_none());
        assert!(qfl.entry_at(4).is_none());
    }

    #[test]
    fn get_next_valid_entry_advances_one_step() {
        let qfl = nav_list(3);
        assert_eq!(get_next_valid_entry(&qfl, 1, crate::vim_defs::Direction::Forward), Some(2));
    }

    #[test]
    fn get_next_valid_entry_stops_at_the_end() {
        let qfl = nav_list(3);
        assert_eq!(get_next_valid_entry(&qfl, 3, crate::vim_defs::Direction::Forward), None);
    }

    #[test]
    fn get_next_valid_entry_skips_invalid_entries() {
        let mut qfl = nav_list(4);
        qfl.qf_entries[1].qf_valid = false;
        qfl.qf_entries[2].qf_valid = false;
        assert_eq!(get_next_valid_entry(&qfl, 1, crate::vim_defs::Direction::Forward), Some(4));
    }

    #[test]
    fn get_next_valid_entry_treats_every_entry_as_valid_when_none_are() {
        // qf_nonevalid means the list has no valid entries at all, in
        // which case navigation must not skip them or it could never
        // move.
        let mut qfl = nav_list(3);
        for e in &mut qfl.qf_entries {
            e.qf_valid = false;
        }
        qfl.qf_nonevalid = true;
        assert_eq!(get_next_valid_entry(&qfl, 1, crate::vim_defs::Direction::Forward), Some(2));
    }

    #[test]
    fn get_next_valid_entry_forward_file_skips_the_starting_file() {
        let mut qfl = nav_list(4);
        // Entries 1 and 2 share a file; ForwardFile must skip past it.
        qfl.qf_entries[1].qf_fnum = qfl.qf_entries[0].qf_fnum;
        assert_eq!(
            get_next_valid_entry(&qfl, 1, crate::vim_defs::Direction::ForwardFile),
            Some(3)
        );
        // Plain Forward stops at the very next entry instead.
        assert_eq!(get_next_valid_entry(&qfl, 1, crate::vim_defs::Direction::Forward), Some(2));
    }

    #[test]
    fn get_prev_valid_entry_mirrors_the_forward_walk() {
        let qfl = nav_list(3);
        assert_eq!(get_prev_valid_entry(&qfl, 3, crate::vim_defs::Direction::Backward), Some(2));
        assert_eq!(get_prev_valid_entry(&qfl, 1, crate::vim_defs::Direction::Backward), None);
    }

    #[test]
    fn get_prev_valid_entry_skips_invalid_and_same_file_entries() {
        let mut qfl = nav_list(4);
        qfl.qf_entries[2].qf_valid = false;
        assert_eq!(get_prev_valid_entry(&qfl, 4, crate::vim_defs::Direction::Backward), Some(2));

        let mut qfl = nav_list(4);
        qfl.qf_entries[2].qf_fnum = qfl.qf_entries[3].qf_fnum;
        assert_eq!(
            get_prev_valid_entry(&qfl, 4, crate::vim_defs::Direction::BackwardFile),
            Some(2)
        );
    }

    #[test]
    fn get_nth_entry_moves_to_the_requested_number() {
        let qfl = nav_list(5);
        assert_eq!(get_nth_entry(&qfl, 4), 4);

        let mut qfl = nav_list(5);
        qfl.qf_index = 5;
        assert_eq!(get_nth_entry(&qfl, 2), 2);
    }

    #[test]
    fn get_nth_entry_clamps_out_of_range_requests() {
        let qfl = nav_list(3);
        // Past the end clamps to the last entry rather than failing.
        assert_eq!(get_nth_entry(&qfl, 99), 3);
        // Below the first clamps to 1, since entries are 1-based.
        assert_eq!(get_nth_entry(&qfl, -5), 1);
        assert_eq!(get_nth_entry(&qfl, 0), 1);
    }

    #[test]
    fn qf_alloc_stack_builds_an_empty_stack_of_the_requested_size() {
        let qi = qf_alloc_stack(QfltypeT::Quickfix, 10);
        assert_eq!(qi.qf_maxcount, 10);
        assert_eq!(qi.qf_lists.len(), 10);
        // Allocated but not yet populated: no lists are live.
        assert_eq!(qi.qf_listcount, 0);
        assert_eq!(qi.qf_curlist, 0);
        assert_eq!(qi.qf_bufnr, INVALID_QFBUFNR);
        assert!(is_qf_stack(&qi));
    }

    #[test]
    fn qf_alloc_stack_only_refcounts_location_lists() {
        // The quickfix stack is a static singleton in the original, so
        // it is never reference-counted; a location list is.
        assert_eq!(qf_alloc_stack(QfltypeT::Quickfix, 1).qf_refcount, 0);
        assert_eq!(qf_alloc_stack(QfltypeT::Location, 1).qf_refcount, 1);
    }

    #[test]
    fn qf_alloc_stack_accepts_a_zero_size() {
        let qi = qf_alloc_stack(QfltypeT::Quickfix, 0);
        assert!(qi.qf_lists.is_empty());
        assert!(qf_stack_empty(Some(&qi)));
    }

    #[test]
    fn qf_free_list_stack_items_frees_only_the_live_lists() {
        let mut qi = stack_with(3);
        for (i, qfl) in qi.qf_lists.iter_mut().enumerate() {
            qfl.qf_id = u32::try_from(i).unwrap() + 1;
            qfl.qf_title = Some(b"t".to_vec());
            qfl.qf_entries.push(QflineT::default());
        }
        // Only the first two are live.
        qi.qf_listcount = 2;

        qf_free_list_stack_items(&mut qi);

        assert_eq!(qi.qf_lists[0].qf_id, 0);
        assert_eq!(qi.qf_lists[0].qf_title, None);
        assert_eq!(qi.qf_lists[1].qf_count(), 0);
        // The slot past qf_listcount is untouched.
        assert_eq!(qi.qf_lists[2].qf_id, 3);
        assert_eq!(qi.qf_lists[2].qf_title.as_deref(), Some(&b"t"[..]));
        // The stack itself survives - only its lists are freed.
        assert_eq!(qi.qf_lists.len(), 3);
    }

    #[test]
    fn qf_pop_stack_drops_the_oldest_list_and_keeps_the_array_length() {
        let _lock = crate::globals::global_state_test_lock();
        let mut qi = stack_with(3);
        qi.qf_lists[0].qf_id = 11;
        qi.qf_lists[1].qf_id = 22;
        qi.qf_lists[2].qf_id = 33;

        qf_pop_stack(&mut qi, false);

        // The remaining lists shift down...
        assert_eq!(qi.qf_lists[0].qf_id, 22);
        assert_eq!(qi.qf_lists[1].qf_id, 33);
        // ...and the freed top slot is zeroed but still there, since
        // the original works inside a fixed qf_maxcount allocation.
        assert_eq!(qi.qf_lists.len(), 3);
        assert_eq!(qi.qf_lists[2].qf_id, 0);
        // Without `adjust`, the counts are left to the caller.
        assert_eq!(qi.qf_listcount, 3);
    }

    #[test]
    fn qf_pop_stack_adjust_moves_curlist_down() {
        let _lock = crate::globals::global_state_test_lock();
        let mut qi = stack_with(3);
        qi.qf_curlist = 2;

        qf_pop_stack(&mut qi, true);

        assert_eq!(qi.qf_listcount, 2);
        assert_eq!(qi.qf_curlist, 1);
    }

    #[test]
    fn qf_pop_stack_adjust_from_the_oldest_jumps_to_the_newest() {
        let _lock = crate::globals::global_state_test_lock();
        let mut qi = stack_with(3);
        // The list being removed IS the current one, so the original
        // points at the newest remaining list instead of at -1.
        qi.qf_curlist = 0;

        qf_pop_stack(&mut qi, true);

        assert_eq!(qi.qf_listcount, 2);
        assert_eq!(qi.qf_curlist, 1);
    }

    #[test]
    fn qf_new_list_appends_and_hands_out_increasing_ids() {
        let _lock = crate::globals::global_state_test_lock();
        let _ids = LastQfIdGuard::new();
        let mut qi = stack_with(3);
        qi.qf_listcount = 0;
        qi.qf_maxcount = 3;

        unsafe { qf_new_list(&mut qi, Some(b"first")) };
        assert_eq!(qi.qf_curlist, 0);
        assert_eq!(qi.qf_listcount, 1);
        let first_id = qi.qf_lists[0].qf_id;
        assert_eq!(qi.qf_lists[0].qf_title.as_deref(), Some(&b"first"[..]));

        unsafe { qf_new_list(&mut qi, Some(b"second")) };
        assert_eq!(qi.qf_curlist, 1);
        assert_eq!(qi.qf_listcount, 2);
        assert_eq!(qi.qf_lists[1].qf_id, first_id + 1);
    }

    #[test]
    fn qf_new_list_inherits_the_stacks_own_type() {
        let _lock = crate::globals::global_state_test_lock();
        let _ids = LastQfIdGuard::new();
        let mut qi = stack_with(2);
        qi.qf_listcount = 0;
        qi.qf_maxcount = 2;
        qi.qfl_type = QfltypeT::Location;

        unsafe { qf_new_list(&mut qi, None) };
        assert!(is_ll_list(&qi.qf_lists[0]));
    }

    #[test]
    fn qf_new_list_discards_lists_above_the_current_one() {
        let _lock = crate::globals::global_state_test_lock();
        let _ids = LastQfIdGuard::new();
        let mut qi = stack_with(4);
        qi.qf_maxcount = 4;
        qi.qf_listcount = 3;
        qi.qf_lists[2].qf_id = 99;
        // Browsing back to the first list and starting a new one
        // replaces the abandoned branch rather than growing past it.
        qi.qf_curlist = 0;

        unsafe { qf_new_list(&mut qi, Some(b"new")) };

        assert_eq!(qi.qf_listcount, 2);
        assert_eq!(qi.qf_curlist, 1);
        assert_eq!(qi.qf_lists[1].qf_title.as_deref(), Some(&b"new"[..]));
        // The discarded list is gone, not merely shadowed.
        assert_ne!(qi.qf_lists[1].qf_id, 99);
    }

    #[test]
    fn qf_new_list_on_a_full_stack_drops_the_oldest() {
        let _lock = crate::globals::global_state_test_lock();
        let _ids = LastQfIdGuard::new();
        let mut qi = stack_with(2);
        qi.qf_maxcount = 2;
        qi.qf_listcount = 2;
        qi.qf_curlist = 1;
        qi.qf_lists[0].qf_id = 11;
        qi.qf_lists[1].qf_id = 22;

        unsafe { qf_new_list(&mut qi, Some(b"newest")) };

        // Count stays at the cap, the oldest is gone, and the newest
        // list occupies the top slot.
        assert_eq!(qi.qf_listcount, 2);
        assert_eq!(qi.qf_curlist, 1);
        assert_eq!(qi.qf_lists[0].qf_id, 22);
        assert_eq!(qi.qf_lists[1].qf_title.as_deref(), Some(&b"newest"[..]));
    }

    #[test]
    fn qf_free_items_clears_entries_and_parse_state() {
        let mut qfl = QfListT {
            qf_index: 3,
            qf_nonevalid: false,
            qf_directory: Some(b"/tmp".to_vec()),
            qf_currfile: Some(b"a.c".to_vec()),
            qf_multiline: true,
            qf_multiignore: true,
            qf_multiscan: true,
            ..list_with(4)
        };

        qf_free_items(&mut qfl);

        assert_eq!(qfl.qf_count(), 0);
        assert_eq!(qfl.qf_index, 0);
        // An emptied list has, by definition, no valid entries.
        assert!(qfl.qf_nonevalid);
        assert_eq!(qfl.qf_directory, None);
        assert_eq!(qfl.qf_currfile, None);
        assert!(!qfl.qf_multiline);
        assert!(!qfl.qf_multiignore);
        assert!(!qfl.qf_multiscan);
    }

    #[test]
    fn qf_free_items_leaves_the_title_and_context_alone() {
        // The original is explicit that these survive qf_free_items
        // and are only cleared by qf_free.
        let mut qfl = QfListT {
            qf_title: Some(b"keep me".to_vec()),
            qf_ctx: Some(Box::new(crate::eval::typval_defs::TypvalT::default())),
            qf_id: 5,
            qf_changedtick: 9,
            ..list_with(2)
        };

        qf_free_items(&mut qfl);

        assert_eq!(qfl.qf_title.as_deref(), Some(&b"keep me"[..]));
        assert!(qfl.qf_ctx.is_some());
        assert_eq!(qfl.qf_id, 5);
        assert_eq!(qfl.qf_changedtick, 9);
    }

    #[test]
    fn qf_free_also_clears_the_title_context_and_id() {
        let mut qfl = QfListT {
            qf_title: Some(b"gone".to_vec()),
            qf_ctx: Some(Box::new(crate::eval::typval_defs::TypvalT::default())),
            qf_id: 5,
            qf_changedtick: 9,
            ..list_with(2)
        };

        qf_free(&mut qfl);

        assert_eq!(qfl.qf_count(), 0);
        assert_eq!(qfl.qf_title, None);
        assert!(qfl.qf_ctx.is_none());
        assert_eq!(qfl.qf_id, 0);
        assert_eq!(qfl.qf_changedtick, 0);
    }

    #[test]
    fn qf_free_is_safe_on_an_already_empty_list() {
        let mut qfl = QfListT::default();
        qf_free(&mut qfl);
        assert_eq!(qfl.qf_count(), 0);
        assert!(qfl.qf_nonevalid);
    }

    #[test]
    fn qf_list_changed_increments_the_tick_once() {
        let mut qfl = QfListT {
            qf_changedtick: 41,
            ..Default::default()
        };

        qf_list_changed(&mut qfl);

        assert_eq!(qfl.qf_changedtick, 42);
    }

    #[test]
    fn qf_list_changed_accumulates_multiple_updates() {
        let mut qfl = QfListT::default();

        qf_list_changed(&mut qfl);
        qf_list_changed(&mut qfl);
        qf_list_changed(&mut qfl);

        assert_eq!(qfl.qf_changedtick, 3);
    }

    #[test]
    fn qf_id2nr_finds_a_list_by_its_unique_id() {
        let mut qi = stack_with(3);
        qi.qf_lists[0].qf_id = 11;
        qi.qf_lists[1].qf_id = 22;
        qi.qf_lists[2].qf_id = 33;

        assert_eq!(qf_id2nr(&qi, 11), 0);
        assert_eq!(qf_id2nr(&qi, 22), 1);
        assert_eq!(qf_id2nr(&qi, 33), 2);
    }

    #[test]
    fn qf_id2nr_returns_invalid_for_an_unknown_id() {
        let mut qi = stack_with(2);
        qi.qf_lists[0].qf_id = 11;
        qi.qf_lists[1].qf_id = 22;
        assert_eq!(qf_id2nr(&qi, 99), INVALID_QFIDX);
        assert_eq!(qf_id2nr(&crate::types_defs::QfInfoT::default(), 11), INVALID_QFIDX);
    }

    #[test]
    fn qf_id2nr_ignores_lists_beyond_qf_listcount() {
        // The original loops to qf_listcount, not to the array's own
        // size, so a matching id past the live range is not found.
        let mut qi = stack_with(3);
        qi.qf_lists[2].qf_id = 33;
        qi.qf_listcount = 2;
        assert_eq!(qf_id2nr(&qi, 33), INVALID_QFIDX);
    }

    #[test]
    fn qf_restore_list_is_a_noop_when_the_saved_list_is_current() {
        let mut qi = stack_with(2);
        qi.qf_lists[0].qf_id = 11;
        qi.qf_lists[1].qf_id = 22;
        qi.qf_curlist = 1;

        assert_eq!(qf_restore_list(&mut qi, 22), crate::vim_defs::OK);
        assert_eq!(qi.qf_curlist, 1);
    }

    #[test]
    fn qf_restore_list_selects_the_saved_list_by_id() {
        let mut qi = stack_with(3);
        qi.qf_lists[0].qf_id = 11;
        qi.qf_lists[1].qf_id = 22;
        qi.qf_lists[2].qf_id = 33;
        qi.qf_curlist = 0;

        assert_eq!(qf_restore_list(&mut qi, 33), crate::vim_defs::OK);
        assert_eq!(qi.qf_curlist, 2);
    }

    #[test]
    fn qf_restore_list_fails_without_changing_selection_when_id_is_gone() {
        let mut qi = stack_with(2);
        qi.qf_lists[0].qf_id = 11;
        qi.qf_lists[1].qf_id = 22;
        qi.qf_curlist = 1;

        assert_eq!(qf_restore_list(&mut qi, 99), crate::vim_defs::FAIL);
        assert_eq!(qi.qf_curlist, 1);
    }

    #[test]
    fn qf_store_title_stores_the_title_verbatim() {
        // Verified against a real nvim: setqflist(.., {'title':
        // 'mytitle'}) reports back exactly "mytitle" with no ':'
        // prefix, so this function does NOT prepend one despite what
        // the original's own doc comment claims.
        let mut qfl = QfListT::default();
        qf_store_title(&mut qfl, Some(b"mytitle"));
        assert_eq!(qfl.qf_title.as_deref(), Some(&b"mytitle"[..]));
    }

    #[test]
    fn qf_store_title_none_clears_the_previous_title() {
        let mut qfl = QfListT::default();
        qf_store_title(&mut qfl, Some(b"first"));
        qf_store_title(&mut qfl, None);
        assert_eq!(qfl.qf_title, None);
    }

    #[test]
    fn qf_store_title_replaces_rather_than_appends() {
        let mut qfl = QfListT::default();
        qf_store_title(&mut qfl, Some(b"first"));
        qf_store_title(&mut qfl, Some(b"second"));
        assert_eq!(qfl.qf_title.as_deref(), Some(&b"second"[..]));
    }

    #[test]
    fn qf_store_title_accepts_an_empty_title() {
        // Real nvim reports an empty title back as empty, NOT as
        // absent, so an empty slice must not collapse into None.
        let mut qfl = QfListT::default();
        qf_store_title(&mut qfl, Some(b""));
        assert_eq!(qfl.qf_title.as_deref(), Some(&b""[..]));
    }

    #[test]
    fn qf_cmdtitle_prepends_a_colon() {
        // Verified against a real nvim: `cexpr! []` produces the
        // title ":cexpr! []".
        assert_eq!(qf_cmdtitle(b"cexpr! []"), b":cexpr! []".to_vec());
        assert_eq!(qf_cmdtitle(b""), b":".to_vec());
    }

    #[test]
    fn qf_cmdtitle_truncates_at_iosize() {
        let long = vec![b'x'; crate::globals::IOSIZE * 2];
        let title = qf_cmdtitle(&long);
        // snprintf writes at most IOSIZE-1 bytes plus its own NUL,
        // which this owned buffer does not carry.
        assert_eq!(title.len(), crate::globals::IOSIZE - 1);
        assert_eq!(title[0], b':');
    }

    /// Restores `QFGA` after a test, so its shared state cannot leak.
    struct QfgaGuard;
    impl Drop for QfgaGuard {
        fn drop(&mut self) {
            let ga = unsafe { QFGA.get_mut() };
            ga.ga_clear();
            ga.ga_init(1, 256);
        }
    }

    #[test]
    fn qfga_get_hands_back_an_empty_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = QfgaGuard;

        let ga = unsafe { qfga_get() };
        ga.ga_len = 17;
        // Getting it again resets the length, which is what makes it
        // safe to share between commands.
        assert_eq!(unsafe { qfga_get() }.ga_len, 0);
    }

    #[test]
    fn qfga_clear_keeps_a_small_buffer_allocated() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = QfgaGuard;

        let ga = unsafe { qfga_get() };
        ga.ga_maxlen = 256;
        ga.ga_len = 42;

        unsafe { qfga_clear() };
        let ga = unsafe { QFGA.get_mut() };
        assert_eq!(ga.ga_len, 0);
        // Capacity is retained - the whole reason the buffer is shared.
        assert_eq!(ga.ga_maxlen, 256);
    }

    #[test]
    fn qfga_clear_frees_a_large_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = QfgaGuard;

        let ga = unsafe { qfga_get() };
        ga.ga_data = vec![0; 2000];
        ga.ga_maxlen = 2000;
        ga.ga_len = 2000;

        unsafe { qfga_clear() };
        let ga = unsafe { QFGA.get_mut() };
        // Over the 1000-byte threshold the memory is handed back
        // rather than held between commands.
        assert_eq!(ga.ga_maxlen, 0);
        assert!(ga.ga_data.is_empty());
    }

    #[test]
    fn qfga_clear_threshold_is_exclusive_at_1000() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = QfgaGuard;

        // The original tests `> 1000`, so exactly 1000 is retained.
        let ga = unsafe { qfga_get() };
        ga.ga_maxlen = 1000;
        unsafe { qfga_clear() };
        assert_eq!(unsafe { QFGA.get_mut() }.ga_maxlen, 1000);

        let ga = unsafe { QFGA.get_mut() };
        ga.ga_maxlen = 1001;
        unsafe { qfga_clear() };
        assert_eq!(unsafe { QFGA.get_mut() }.ga_maxlen, 0);
    }

    #[test]
    fn always_false_for_the_quickfix_list_when_b_has_qf_entry_is_unset() {
        let buf = BufT::default();
        assert!(!qf_mark_adjust(&buf, None, 1, 5, 2, 0));
    }

    #[test]
    fn always_false_for_a_location_list_when_b_has_qf_entry_is_unset() {
        let buf = BufT::default();
        let wp = WinT::default();
        assert!(!qf_mark_adjust(&buf, Some(&wp), 1, 5, 2, 0));
    }

    #[test]
    fn checks_the_ll_entry_flag_specifically_when_wp_is_some() {
        // BUF_HAS_QF_ENTRY alone set, but wp is Some (location list) -
        // must check BUF_HAS_LL_ENTRY, not BUF_HAS_QF_ENTRY, so this
        // still hits the real "no ll entry" false-return fast path.
        let buf = BufT { b_has_qf_entry: BUF_HAS_QF_ENTRY, ..BufT::default() };
        let wp = WinT::default();
        assert!(!qf_mark_adjust(&buf, Some(&wp), 1, 5, 2, 0));
    }

    #[test]
    fn checks_the_qf_entry_flag_specifically_when_wp_is_none() {
        // BUF_HAS_LL_ENTRY alone set, but wp is None (quickfix list) -
        // must check BUF_HAS_QF_ENTRY, not BUF_HAS_LL_ENTRY.
        let buf = BufT { b_has_qf_entry: BUF_HAS_LL_ENTRY, ..BufT::default() };
        assert!(!qf_mark_adjust(&buf, None, 1, 5, 2, 0));
    }

    #[test]
    #[should_panic(expected = "qf_mark_adjust's real entry-adjustment body is unreachable today")]
    fn panics_if_the_real_flag_is_ever_actually_set() {
        // Demonstrates precisely where the real, not-yet-translated
        // body would begin, in case a future change ever lets
        // something set this flag for real.
        let buf = BufT { b_has_qf_entry: BUF_HAS_QF_ENTRY, ..BufT::default() };
        let _ = qf_mark_adjust(&buf, None, 1, 5, 2, 0);
    }

    // --- qf_fmt_text ---

    fn fmt(text: &[u8]) -> Vec<u8> {
        let mut gap = GarrayT::new(1, 4);
        qf_fmt_text(&mut gap, text);
        gap.ga_data[..gap.ga_len as usize].to_vec()
    }

    #[test]
    fn qf_fmt_text_empty_is_unchanged() {
        assert_eq!(fmt(b""), b"");
    }

    #[test]
    fn qf_fmt_text_plain_text_with_no_newline_is_unchanged() {
        assert_eq!(fmt(b"hello world"), b"hello world");
    }

    #[test]
    fn qf_fmt_text_single_newline_becomes_a_space() {
        assert_eq!(fmt(b"foo\nbar"), b"foo bar");
    }

    #[test]
    fn qf_fmt_text_newline_followed_by_spaces_collapses_to_one_space() {
        assert_eq!(fmt(b"foo\n   bar"), b"foo bar");
    }

    #[test]
    fn qf_fmt_text_multiple_consecutive_newlines_collapse_to_one_space() {
        assert_eq!(fmt(b"foo\n\n\nbar"), b"foo bar");
    }

    #[test]
    fn qf_fmt_text_trailing_newline_with_nothing_after() {
        assert_eq!(fmt(b"foo\n"), b"foo ");
    }

    #[test]
    fn qf_fmt_text_leading_newline() {
        assert_eq!(fmt(b"\nfoo"), b" foo");
    }

    #[test]
    fn qf_fmt_text_mixed_whitespace_and_newline_run_all_absorbed() {
        // space, tab, newline, space - all whitespace-or-newline, so
        // ALL are absorbed after the first newline triggers the
        // single space append.
        assert_eq!(fmt(b"a\n \t\n b"), b"a b");
    }

    #[test]
    fn qf_fmt_text_tab_alone_is_preserved_as_is_when_not_after_a_newline() {
        // A tab NOT immediately following a newline is just a plain
        // byte, copied through unchanged (only whitespace immediately
        // AFTER a newline gets absorbed).
        assert_eq!(fmt(b"a\tb"), b"a\tb");
    }
}
