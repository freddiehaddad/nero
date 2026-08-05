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
