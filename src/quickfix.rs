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
/// belonging to `qf_init_stack`/the per-window `w_llist` fields,
/// neither of which is translated yet, so it is left to the caller
/// here. The refcount difference between the two IS preserved, since
/// it is part of the returned value rather than of where it lives.
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

#[cfg(test)]
mod tests {
    use super::*;

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
