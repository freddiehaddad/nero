//! Translated from `src/nvim/decoration.c` (tractable core only).
//!
//! `decoration.c` (~2000 lines) is neovim's extmark-decoration
//! rendering engine (virtual text, highlights, conceal, sign columns,
//! decoration providers) - a substantial subsystem of its own, almost
//! entirely dependent on the marktree query machinery and the Lua
//! decoration-provider callback host, not attempted here.
//!
//! Translated: [`decor_conceal_line`]/[`decor_virt_lines`] - real,
//! faithful translations of each function's own real, always-taken
//! early-return path, matching this session's established "translate
//! the real condition, not a hardcoded shortcut" pattern:
//! - [`decor_conceal_line`]: its own FIRST `||` operand,
//!   `wp.w_onebuf_opt.wo_cole < 2`, is always true today (nothing in
//!   this crate can currently raise `'conceallevel'` above its real
//!   default of `0` - the options-parsing engine isn't built), so due
//!   to `||` short-circuit evaluation, this function always returns
//!   `false` without ever touching `conceal_cursor_line`/
//!   `buf_meta_total`/the marktree at all.
//! - [`decor_virt_lines`]: its own first check,
//!   `!buf_meta_total(buf, kMTMetaLines)`, is always true today
//!   (nothing in this crate can currently attach virtual lines to any
//!   buffer - the extmark-creation API isn't reachable), so this
//!   function always returns `0` immediately without touching its
//!   `num_below`/`lines` out-parameters or the marktree at all.
//!
//! Also translated: [`win_lines_concealed`] - fully real and complete
//! (not a "real early-return path" translation like the two above),
//! since its only two dependencies, `crate::fold::has_any_folding`
//! and `wp.w_onebuf_opt.wo_cole`, are both already real. Used by
//! `move.c`'s `check_top_offset`.
//!
//! Also translated: [`DecorRange`] (with [`DecorRangeKind`] and
//! [`DecorRangeData`], from `decoration.h`) plus the two predicates
//! it unblocks, [`decor_virt_pos`] and [`decor_virt_pos_kind`].
//! Translated ahead of the redraw pass that populates them, since
//! `DecorRange` is the type gating most of this file's remaining
//! functions.
//!
//! Also translated: [`decor_put_vt`] (heap-allocating a virtual-text
//! node and linking it ahead of an existing chain) and
//! [`decor_virt_line_wrap`] (whether a virtual line wraps onto extra
//! rows).
//!
//! Also translated: [`sign_item_cmp`], the comparator ordering the
//! signs shown on one line, and [`may_force_numberwidth_recompute`],
//! which invalidates the cached number-column width in every window
//! whose `'signcolumn'` is `"number"` and shows the changed buffer.
//!
//! Also translated: [`DecorState`] (with [`DecorRangeSlot`], from
//! `decoration.h`) and its `decor_state` file-static, plus the two
//! functions they unblock, [`decor_state_invalidate`] and
//! [`decor_redraw_end`].
//!
//! Deferred: everything else in the file - real virtual-text/
//! highlight/conceal rendering, needing the marktree query machinery
//! and decoration-provider Lua callbacks, neither translated.

use crate::buffer::buf_meta_total;
use crate::buffer_defs::BufT;
use crate::buffer_defs::WinT;
use crate::decoration_defs::VirtLines;
use crate::types_defs::TriState;
use crate::marktree_defs::MetaIndex;

/// Which flavour of decoration a [`DecorRange`] carries
/// (`DecorRangeKindEnum`/`DecorRangeKind`).
///
/// The original stores this as a separate `uint8_t` tag beside an
/// untagged union, which lets the two disagree. Here it is derived
/// from the payload instead (see [`DecorRange::kind`]), so they
/// cannot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecorRangeKind {
    Highlight,
    Sign,
    VirtText,
    VirtLines,
    UIWatched,
}

/// The payload of a [`DecorRange`] (the original's `data` union).
///
/// Modeled as a safe tagged enum, matching
/// `decoration_defs.rs`'s own `DecorVirtTextEnumData` precedent:
/// a `DecorRange` is never stored compactly inline in the marktree,
/// so there is no memory-layout reason to prefer an untagged union.
///
/// Note the original's `kind` tag is NOT one-to-one with its union
/// members - `kDecorKindHighlight` and `kDecorKindSign` both read
/// `data.sh`, and `kDecorKindVirtText` and `kDecorKindVirtLines` both
/// read `data.vt`. This enum keeps all five cases distinct so the
/// tag can be derived rather than tracked separately, which is what
/// makes the two impossible to desynchronize.
#[derive(Debug, Clone)]
pub enum DecorRangeData {
    /// `kDecorKindHighlight`, reading the original's `data.sh`.
    Highlight(crate::decoration_defs::DecorSignHighlight),
    /// `kDecorKindSign`, reading the original's `data.sh`.
    Sign(crate::decoration_defs::DecorSignHighlight),
    /// `kDecorKindVirtText`, reading the original's `data.vt`.
    VirtText(*mut crate::decoration_defs::DecorVirtText),
    /// `kDecorKindVirtLines`, reading the original's `data.vt`.
    VirtLines(*mut crate::decoration_defs::DecorVirtText),
    /// `kDecorKindUIWatched`, reading the original's `data.ui`.
    UIWatched {
        ns_id: u32,
        mark_id: u32,
        pos: crate::decoration_defs::VirtTextPos,
    },
}

/// `draw_col`: draw the virtual text on the current screen line after
/// deciding where.
pub const DECOR_DRAW_COL_UNDECIDED: i32 = -1;
/// `draw_col`: the virtual text may be drawn at a position yet to be
/// assigned.
pub const DECOR_DRAW_COL_DEFERRED: i32 = -3;
/// `draw_col`: the virtual text has just been added.
pub const DECOR_DRAW_COL_JUST_ADDED: i32 = -10;

/// One decoration active over a screen range, as collected by the
/// redraw pass (`DecorRange`).
///
/// The `vt` pointer inside [`DecorRangeData`] is a borrowed raw
/// pointer into decoration storage, matching the original: it stays a
/// raw pointer per this crate's convention for aliasing cases. The
/// original's own warning applies unchanged - the `next` pointer of a
/// borrowed `DecorVirtText` MUST NOT be followed from here, because
/// these are separate ranges and `vt->next` may already point into
/// freelist memory.
#[derive(Debug, Clone)]
pub struct DecorRange {
    pub start_row: i32,
    pub start_col: i32,
    pub end_row: i32,
    pub end_col: i32,
    /// range insertion order (`ordering`).
    pub ordering: i32,
    pub priority_internal: crate::decoration_defs::DecorPriorityInternal,
    /// ephemeral decoration, free memory immediately (`owned`).
    pub owned: bool,
    /// the decoration itself; also carries the original's `kind` tag
    /// (see [`DecorRange::kind`]).
    pub data: DecorRangeData,
    /// cached lookup of `inl.hl_id` if it was a highlight (`attr_id`).
    pub attr_id: i32,
    /// Screen column to draw the virtual text; see the
    /// `DECOR_DRAW_COL_*` constants for the negative sentinels, and
    /// `i32::MIN` for "should no longer be drawn" (`draw_col`).
    pub draw_col: i32,
}

impl DecorRange {
    /// The original's `kind` field, derived from the payload instead
    /// of stored alongside it.
    #[must_use]
    pub fn kind(&self) -> DecorRangeKind {
        match self.data {
            DecorRangeData::Highlight(_) => DecorRangeKind::Highlight,
            DecorRangeData::Sign(_) => DecorRangeKind::Sign,
            DecorRangeData::VirtText(_) => DecorRangeKind::VirtText,
            DecorRangeData::VirtLines(_) => DecorRangeKind::VirtLines,
            DecorRangeData::UIWatched { .. } => DecorRangeKind::UIWatched,
        }
    }
}

/// Whether `decor` has a virtual position - i.e. is virtual text or
/// a UI-watched mark (`decor_virt_pos`).
#[must_use]
pub fn decor_virt_pos(decor: &DecorRange) -> bool {
    matches!(
        decor.kind(),
        DecorRangeKind::VirtText | DecorRangeKind::UIWatched
    )
}

/// Where `decor`'s virtual text is positioned relative to the line
/// (`decor_virt_pos_kind`).
///
/// Decorations with no virtual position at all report
/// [`crate::decoration_defs::VirtTextPos::EndOfLine`]; the original
/// notes that value is never used for them and is just "whatever".
/// Note virtual LINES take that fallback too, even though they carry
/// the same `DecorVirtText` payload as virtual text - the original
/// tests `kind == kDecorKindVirtText` specifically, so reading `pos`
/// off the payload whenever one is present would be wrong.
///
/// # Safety
/// If `decor` is virtual text, its borrowed `DecorVirtText` pointer
/// must be valid.
#[must_use]
pub unsafe fn decor_virt_pos_kind(decor: &DecorRange) -> crate::decoration_defs::VirtTextPos {
    match decor.data {
        // SAFETY: forwarded from this function's own safety doc.
        DecorRangeData::VirtText(vt) => unsafe { (*vt).pos },
        DecorRangeData::UIWatched { pos, .. } => pos,
        _ => crate::decoration_defs::VirtTextPos::EndOfLine,
    }
}

/// One slot in [`DecorState`]'s range storage (`DecorRangeSlot`).
///
/// Ranges can be removed in any order, so freed slots are tracked
/// with a freelist chained through the slot itself; the head index
/// lives in `DecorState::free_slot_i`.
///
/// The original overlays the two uses in an untagged union. A safe
/// tagged enum is used here for the same reason as
/// [`DecorRangeData`]: these slots live in a plain growable vector,
/// never packed into the marktree, so nothing depends on their
/// layout.
#[derive(Debug, Clone)]
pub enum DecorRangeSlot {
    /// An occupied slot holding a live range (`range`).
    Range(DecorRange),
    /// A freed slot; holds the index of the next free slot, or
    /// [`DECOR_NO_FREE_SLOT`] at the end of the chain (`next_free_i`).
    Free(i32),
}

/// `free_slot_i`/`next_free_i` sentinel meaning "no freed slots".
pub const DECOR_NO_FREE_SLOT: i32 = -1;

/// The decoration state built up while redrawing one window
/// (`DecorState`).
///
/// `itr` is a one-element array in the original purely so it can be
/// passed as a pointer while remaining an embedded member; a plain
/// field achieves the same in Rust, matching
/// `marktree_defs.rs`'s own treatment of `MarkTree::id2node`.
#[derive(Debug)]
pub struct DecorState {
    pub itr: crate::marktree_defs::MarkTreeIter,
    pub slots: Vec<DecorRangeSlot>,
    /// Indices into `slots`. Entries in `[0, current_end)` point to
    /// ranges starting before the current position, sorted by
    /// priority then insertion order; entries in
    /// `[future_begin, ranges_i.len())` point to ranges starting
    /// after it, sorted by starting position (`ranges_i`).
    pub ranges_i: Vec<i32>,
    pub current_end: i32,
    pub future_begin: i32,
    /// Head of the [`DecorRangeSlot::Free`] chain, or
    /// [`DECOR_NO_FREE_SLOT`] if none are freed (`free_slot_i`).
    pub free_slot_i: i32,
    /// Counter used to keep track of range insertion order
    /// (`new_range_ordering`).
    pub new_range_ordering: i32,
    pub win: *mut WinT,
    pub top_row: i32,
    pub row: i32,
    pub col_last: i32,
    pub current: i32,
    pub eol_col: i32,
    pub conceal: i32,
    pub conceal_char: crate::types_defs::ScharT,
    pub conceal_attr: i32,
    pub spell: TriState,
    pub running_decor_provider: bool,
    pub itr_valid: bool,
}

impl Default for DecorState {
    /// The original's `decor_state INIT( = { 0 })` - every field
    /// zeroed, which for `free_slot_i` means slot 0, NOT
    /// [`DECOR_NO_FREE_SLOT`]. That is harmless in the original
    /// because the freelist is only consulted once `slots` is
    /// non-empty, and it is reset properly by the redraw pass.
    ///
    /// Note `spell` is [`TriState::False`] (the zero value) and
    /// deliberately NOT `TriState::default()`, which is
    /// `TriState::None` (`-1`) and would mean "no spell decision
    /// recorded" rather than "spell checking off".
    fn default() -> Self {
        DecorState {
            itr: crate::marktree_defs::MarkTreeIter::default(),
            slots: Vec::new(),
            ranges_i: Vec::new(),
            current_end: 0,
            future_begin: 0,
            free_slot_i: 0,
            new_range_ordering: 0,
            win: std::ptr::null_mut(),
            top_row: 0,
            row: 0,
            col_last: 0,
            current: 0,
            eol_col: 0,
            conceal: 0,
            conceal_char: 0,
            conceal_attr: 0,
            spell: TriState::False,
            running_decor_provider: false,
            itr_valid: false,
        }
    }
}

/// `decor_state` - the decoration state for the window currently
/// being redrawn.
pub static DECOR_STATE: crate::globals::GlobalCell<DecorState> =
    crate::globals::GlobalCell::new(DecorState {
        itr: crate::marktree_defs::MarkTreeIter {
            pos: crate::marktree_defs::MtPos { row: 0, col: 0 },
            lvl: 0,
            x: std::ptr::null_mut(),
            i: 0,
            s: [crate::marktree_defs::MarkTreeIterFrame { oldcol: 0, i: 0 };
                crate::marktree_defs::MT_MAX_DEPTH],
            intersect_idx: 0,
            intersect_pos: crate::marktree_defs::MtPos { row: 0, col: 0 },
            intersect_pos_x: crate::marktree_defs::MtPos { row: 0, col: 0 },
        },
        slots: Vec::new(),
        ranges_i: Vec::new(),
        current_end: 0,
        future_begin: 0,
        free_slot_i: 0,
        new_range_ordering: 0,
        win: std::ptr::null_mut(),
        top_row: 0,
        row: 0,
        col_last: 0,
        current: 0,
        eol_col: 0,
        conceal: 0,
        conceal_char: 0,
        conceal_attr: 0,
        spell: TriState::False,
        running_decor_provider: false,
        itr_valid: false,
    });

/// Global extended sign/highlight storage (`decor_items`).
static DECOR_ITEMS: crate::globals::GlobalCell<Vec<crate::decoration_defs::DecorSignHighlight>> =
    crate::globals::GlobalCell::new(Vec::new());

/// Return the extmark-type bits represented by one inline decoration
/// (`decor_type_flags`).
///
/// # Safety
/// Every non-invalid `sh_idx`/`next` index must address `DECOR_ITEMS`,
/// and decoration state must not be mutated concurrently.
#[must_use]
pub unsafe fn decor_type_flags(decor: &crate::decoration_defs::DecorInline) -> u16 {
    use crate::decoration_defs::{SH_IS_SIGN, VT_IS_LINES};
    use crate::extmark_defs::extmark_type;

    match decor {
        crate::decoration_defs::DecorInline::Highlight(highlight) => {
            if highlight.flags & SH_IS_SIGN != 0 {
                extmark_type::SIGN as u16
            } else {
                extmark_type::HIGHLIGHT as u16
            }
        }
        crate::decoration_defs::DecorInline::Ext(ext) => {
            let mut flags = extmark_type::NONE as u16;
            let mut virt_text = ext.vt.as_deref();
            while let Some(item) = virt_text {
                flags |= if item.flags & VT_IS_LINES != 0 {
                    extmark_type::VIRT_LINES as u16
                } else {
                    extmark_type::VIRT_TEXT as u16
                };
                virt_text = item.next.as_deref();
            }

            let items = unsafe { &*DECOR_ITEMS.as_ptr() };
            let mut index = ext.sh_idx;
            while index != crate::decoration_defs::DECOR_ID_INVALID {
                let highlight = &items[index as usize];
                flags |= if highlight.flags & SH_IS_SIGN != 0 {
                    extmark_type::SIGN as u16
                } else {
                    extmark_type::HIGHLIGHT as u16
                };
                index = highlight.next;
            }
            flags
        }
    }
}

/// Find inline virtual text at `row`, optionally restricted to a
/// namespace (`decor_find_virttext`).
///
/// The original returns a borrowed pointer into marktree-owned
/// decoration storage. `marktree_itr_current` returns an owned key in
/// this translation, so this returns an owned clone instead.
#[must_use]
pub fn decor_find_virttext(
    buf: &BufT,
    row: i32,
    ns_id: u64,
) -> Option<crate::decoration_defs::DecorVirtText> {
    let mut itr = crate::marktree_defs::MarkTreeIter::default();
    crate::marktree::marktree_itr_get(&buf.b_marktree, row, 0, &mut itr);
    loop {
        let mark = crate::marktree::marktree_itr_current(&itr);
        if mark.pos.row < 0 || mark.pos.row > row {
            return None;
        }
        if !crate::marktree::mt_invalid(&mark) {
            let mut decor = crate::marktree::mt_decor_virt(&mark);
            while decor.is_some_and(|item| {
                item.flags & crate::decoration_defs::VT_IS_LINES != 0
            }) {
                decor = decor.and_then(|item| item.next.as_deref());
            }
            if (ns_id == 0 || ns_id == u64::from(mark.ns))
                && let Some(decor) = decor
            {
                return Some(decor.clone());
            }
        }
        crate::marktree::marktree_itr_next(&buf.b_marktree, &mut itr);
    }
}

/// Whether the current row likely has another decoration to process
/// (`decor_has_more_decorations`).
#[must_use]
pub fn decor_has_more_decorations(state: &DecorState, row: i32) -> bool {
    if state.current_end != 0 || state.future_begin != state.ranges_i.len() as i32 {
        return true;
    }
    let key = crate::marktree::marktree_itr_current(&state.itr);
    key.pos.row >= 0 && key.pos.row <= row
}

/// Invalidate the cached marktree iterator if the decoration state is
/// currently pointed at `buf` (`decor_state_invalidate`).
///
/// Only the window's OWN buffer matters: a change to some other
/// buffer cannot disturb this window's iterator, so the state is left
/// alone.
///
/// # Safety
/// `DECOR_STATE.win`, if non-null, must point to a live `WinT` whose
/// `w_buffer` is valid.
pub unsafe fn decor_state_invalidate(buf: *const BufT) {
    // SAFETY: forwarded from this function's own safety doc.
    let state = unsafe { DECOR_STATE.get_mut() };
    if state.win.is_null() {
        return;
    }
    // SAFETY: forwarded from this function's own safety doc; `win`
    // was just checked non-null.
    if std::ptr::eq(unsafe { (*state.win).w_buffer }, buf) {
        state.itr_valid = false;
    }
}

/// Finish the redraw pass for a window, detaching the decoration
/// state from it (`decor_redraw_end`).
///
/// # Safety
/// Touches the `decor_state` file-static.
pub unsafe fn decor_redraw_end(state: &mut DecorState) {
    state.win = std::ptr::null_mut();
}

/// Force a recompute of the number column's width in every window
/// showing `buf`, when placing or unplacing a sign could have changed
/// it (`may_force_numberwidth_recompute`).
///
/// Only windows with `'signcolumn'` set to `"number"` are affected -
/// there the sign shares the number column, so its presence changes
/// the width. `unplace` forces the recompute unconditionally, since
/// removing a sign can only ever shrink the column; when placing, the
/// recompute is only needed while the column is still narrower than
/// two cells.
///
/// The original's `FOR_ALL_TAB_WINDOWS(tp, wp)` walk is reproduced
/// here directly, following this crate's established idiom
/// (`optionstr.rs`/`move.rs`).
///
/// # Safety
/// `GLOBALS.first_tabpage`'s own `tp_next` chain, and each tabpage's
/// window list, must consist of valid, live pointers.
pub unsafe fn may_force_numberwidth_recompute(buf: *const BufT, unplace: bool) {
    // SAFETY: forwarded from this function's own safety doc.
    let mut tp = unsafe { crate::globals::GLOBALS.get_mut() }.first_tabpage;
    while !tp.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        let is_curtab = std::ptr::eq(tp, unsafe { crate::globals::GLOBALS.get_mut() }.curtab);
        let mut wp = if is_curtab {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { crate::globals::GLOBALS.get_mut() }.firstwin
        } else {
            // SAFETY: forwarded from this function's own safety doc.
            unsafe { &*tp }.tp_firstwin
        };
        while !wp.is_null() {
            // SAFETY: forwarded from this function's own safety doc.
            let w = unsafe { &mut *wp };
            if std::ptr::eq(w.w_buffer, buf)
                && w.w_minscwidth == crate::option_vars::SCL_NUM
                && (w.w_onebuf_opt.wo_nu != 0 || w.w_onebuf_opt.wo_rnu != 0)
                && (unplace || w.w_nrwidth_width < 2)
            {
                w.w_nrwidth_line_count = 0;
            }
            // SAFETY: forwarded from this function's own safety doc.
            wp = unsafe { &*wp }.w_next;
        }
        // SAFETY: forwarded from this function's own safety doc.
        tp = unsafe { &*tp }.tp_next;
    }
}

/// Heap-allocate a copy of `vt`, linked ahead of `next`
/// (`decor_put_vt`).
///
/// The original returns a raw `xmalloc`ed pointer the caller becomes
/// responsible for; here ownership is expressed directly, since this
/// crate already models `DecorVirtText::next` as an owning
/// `Option<Box<_>>`.
#[must_use]
pub fn decor_put_vt(
    vt: crate::decoration_defs::DecorVirtText,
    next: Option<Box<crate::decoration_defs::DecorVirtText>>,
) -> Box<crate::decoration_defs::DecorVirtText> {
    let mut decor_alloc = Box::new(vt);
    decor_alloc.next = next;
    decor_alloc
}

/// Whether a virtual line wraps onto extra rows rather than being
/// truncated or scrolled (`decor_virt_line_wrap`).
///
/// `Auto` defers to the window's own `'wrap'`; `Wrap` always wraps.
/// `Trunc` and `Scroll` never do.
#[must_use]
pub fn decor_virt_line_wrap(
    wp: &WinT,
    overflow: crate::decoration_defs::VirtLineOverflow,
) -> bool {
    use crate::decoration_defs::VirtLineOverflow as O;
    overflow == O::Wrap || (overflow == O::Auto && wp.w_onebuf_opt.wo_wrap != 0)
}

/// Comparator ordering the signs shown on one line (`sign_item_cmp`).
///
/// Sorts DESCENDING on all three keys in turn - priority, then id,
/// then `sign_add_id` - so the highest-priority sign is placed first
/// and, among equal priorities, the most recently added sign wins.
/// Note the original's own comparisons are written the "wrong way
/// round" (`s1 < s2 ? 1 : -1`) precisely to get that descending
/// order out of `qsort`.
///
/// Returns a negative/zero/positive `i32`, matching `qsort`'s own
/// convention and this crate's established comparator shape (e.g.
/// `cmdexpand::sort_func_compare`).
///
/// # Panics
/// If either item has no `DecorSignHighlight`. The original
/// dereferences `sh` unconditionally, so a null there is already a
/// contract violation; this makes it a loud one.
#[must_use]
pub fn sign_item_cmp(s1: &crate::sign_defs::SignItem, s2: &crate::sign_defs::SignItem) -> i32 {
    let sh1 = s1.sh.as_ref().expect("sign_item_cmp: SignItem without a DecorSignHighlight");
    let sh2 = s2.sh.as_ref().expect("sign_item_cmp: SignItem without a DecorSignHighlight");

    if sh1.priority != sh2.priority {
        return if sh1.priority < sh2.priority { 1 } else { -1 };
    }
    if s1.id != s2.id {
        return if s1.id < s2.id { 1 } else { -1 };
    }
    if sh1.sign_add_id != sh2.sign_add_id {
        return if sh1.sign_add_id < sh2.sign_add_id { 1 } else { -1 };
    }
    0
}

/// Called by draw, move and plines code to determine whether a line
/// is concealed. Scans the marktree for `conceal_line` marks on `row`
/// and invokes any `_on_conceal_line` decoration provider callbacks,
/// if necessary (`decor_conceal_line`).
///
/// `check_cursor`: if `true`, avoid an early return for an
/// unconcealed cursorline. Accepted for signature fidelity but
/// genuinely unused by the real, always-taken early-return path
/// translated here (see this module's own doc comment) - the clause
/// that reads it is short-circuited away before ever being evaluated.
///
/// # Safety
/// `wp.w_buffer` must be a valid, non-null pointer to a live `BufT`
/// (forwarded to the real marktree-scanning path, unreachable today).
#[must_use]
pub unsafe fn decor_conceal_line(wp: &WinT, row: i32, _check_cursor: bool) -> bool {
    if row < 0 || wp.w_onebuf_opt.wo_cole < 2 {
        return false;
    }
    unimplemented!(
        "decoration::decor_conceal_line: the real marktree-scanning/decoration-provider path is \
         not yet translated - unreachable in practice today since 'conceallevel' can never be \
         raised above its default of 0, see this module's own doc comment"
    );
}

/// Return the number of rows occupied by the virtual lines attached
/// between `start_row` and `end_row` (`decor_virt_lines`).
///
/// `apply_folds`: only count virtual lines that are not in folds.
/// Accepted for signature fidelity but genuinely unused by the real,
/// always-taken early-return path translated here (see this module's
/// own doc comment).
///
/// # Safety
/// `wp.w_buffer` must be a valid, non-null pointer to a live `BufT`.
#[must_use]
pub unsafe fn decor_virt_lines(
    wp: &WinT,
    _start_row: i32,
    _end_row: i32,
    _num_below: Option<&mut i32>,
    _lines: Option<&mut VirtLines>,
    _apply_folds: bool,
) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let buf = unsafe { &*wp.w_buffer };
    if crate::buffer::buf_meta_total(buf, MetaIndex::Lines) == 0 {
        // Only pay for what you use: in case virt_lines feature is
        // not active in a buffer, plines do not need to access the
        // marktree at all.
        return 0;
    }
    unimplemented!(
        "decoration::decor_virt_lines: the real marktree-scanning path is not yet translated - \
         unreachable in practice today since nothing can attach virtual lines to any buffer, \
         see this module's own doc comment"
    );
}

/// Return `true` when `wp` may have concealed lines: either real
/// folds exist, or `'conceallevel'` hides whole lines (`>= 2`)
/// (`win_lines_concealed`). Fully real and complete - needs only
/// already-translated [`crate::fold::has_any_folding`] and
/// `wp.w_onebuf_opt.wo_cole`.
///
/// # Safety
/// Same as [`crate::fold::has_any_folding`].
#[must_use]
pub unsafe fn win_lines_concealed(wp: &WinT) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    (unsafe { crate::fold::has_any_folding(wp) }) || wp.w_onebuf_opt.wo_cole >= 2
}

/// Count the number of signs in a range after adding or removing a
/// sign, or to (re-)initialize a range in `buf.b_signcols.count`
/// (`buf_signcols_count_range`).
///
/// `add` is `1`, `-1` or `0` for an added, deleted or initialized
/// range. `clear` is `False`, `True` or `None` for an added/deleted,
/// cleared, or initialized range.
///
/// # Scope
///
/// The guard is translated in full and is always taken today, so this
/// function is complete as written for every reachable call.
///
/// Its `!buf_meta_total(buf, MetaIndex::SignText)` operand is always
/// true, because nothing translated can attach a sign-text extmark to
/// any buffer yet - the same real "nothing has been registered"
/// condition [`decor_virt_lines`] relies on just above. The
/// `!buf.b_signcols.autom` operand is likewise true for every buffer
/// this crate can build, and `||` short-circuits before either of the
/// later operands is even evaluated.
///
/// The counting body behind that guard is `unimplemented!()`: it needs
/// `marktree_itr_get_overlap`, `marktree_itr_step_overlap` and
/// `marktree_itr_step_out_filter`, none of which are translated (the
/// overlap-iterator variants deferred in `marktree.rs`). It is
/// unreachable while no sign-text extmark can exist.
pub fn buf_signcols_count_range(buf: &mut BufT, row1: i32, row2: i32, add: i32, clear: TriState) {
    if !buf.b_signcols.autom || row2 < row1 || buf_meta_total(buf, MetaIndex::SignText) == 0 {
        return;
    }

    let _ = (add, clear);
    unimplemented!(
        "sign counting needs marktree_itr_get_overlap/_step_overlap/_step_out_filter, \
         not yet translated; unreachable while no sign-text extmark can exist"
    );
}

/// Represents highlight group `hl_id` as either its name or numeric
/// identifier (`hl_group_name`).
///
/// # Safety
/// The name branch reads the global highlight-group table through
/// [`crate::highlight_group::syn_id2name`].
#[must_use]
pub unsafe fn hl_group_name(
    hl_id: i32,
    hl_name: bool,
) -> crate::api::private::defs::Object {
    if hl_name {
        crate::api::private::defs::Object::String(unsafe {
            crate::highlight_group::syn_id2name(hl_id)
        })
    } else {
        crate::api::private::defs::Object::Integer(i64::from(hl_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decor_has_more_detects_active_future_or_iterator_ranges() {
        let mut state = DecorState {
            current_end: 1,
            ..Default::default()
        };
        assert!(decor_has_more_decorations(&state, 0));

        state.current_end = 0;
        state.ranges_i.push(0);
        assert!(decor_has_more_decorations(&state, 0));

        state.future_begin = 1;
        assert!(!decor_has_more_decorations(&state, 0));
    }

    #[test]
    fn decor_type_flags_classifies_inline_and_extended_payloads() {
        let _lock = crate::globals::global_state_test_lock();
        let inline = crate::decoration_defs::DecorInline::Highlight(
            crate::decoration_defs::DecorHighlightInline::default(),
        );
        assert_eq!(
            unsafe { decor_type_flags(&inline) },
            crate::extmark_defs::extmark_type::HIGHLIGHT as u16
        );

        let saved = std::mem::take(unsafe { DECOR_ITEMS.get_mut() });
        let first = crate::decoration_defs::DecorSignHighlight {
            flags: crate::decoration_defs::SH_IS_SIGN,
            next: 1,
            ..Default::default()
        };
        let second = crate::decoration_defs::DecorSignHighlight::default();
        unsafe { DECOR_ITEMS.get_mut() }.extend([first, second]);
        let virt_lines = crate::decoration_defs::DecorVirtText {
            flags: crate::decoration_defs::VT_IS_LINES,
            ..Default::default()
        };
        let virt_text = crate::decoration_defs::DecorVirtText {
            next: Some(Box::new(virt_lines)),
            ..Default::default()
        };
        let extended = crate::decoration_defs::DecorInline::Ext(
            crate::decoration_defs::DecorExt {
                sh_idx: 0,
                vt: Some(Box::new(virt_text)),
            },
        );
        let got = unsafe { decor_type_flags(&extended) };
        *unsafe { DECOR_ITEMS.get_mut() } = saved;

        assert_eq!(
            got,
            (crate::extmark_defs::extmark_type::NONE
                | crate::extmark_defs::extmark_type::SIGN
                | crate::extmark_defs::extmark_type::HIGHLIGHT
                | crate::extmark_defs::extmark_type::VIRT_TEXT
                | crate::extmark_defs::extmark_type::VIRT_LINES) as u16
        );
    }

    #[test]
    fn decor_find_virttext_skips_virtual_lines_and_filters_namespace() {
        let mut buf = BufT::default();
        let text = DecorVirtText {
            col: 7,
            ..Default::default()
        };
        let lines = DecorVirtText {
            flags: crate::decoration_defs::VT_IS_LINES,
            next: Some(Box::new(text)),
            ..Default::default()
        };
        let key = crate::marktree_defs::MtKey {
            pos: crate::marktree_defs::MtPos::new(3, 0),
            ns: 9,
            id: 1,
            flags: crate::marktree::mt_flags(false, false, false, true),
            decor_data: crate::decoration_defs::DecorInlineData {
                ext: std::mem::ManuallyDrop::new(crate::decoration_defs::DecorExt {
                    sh_idx: crate::decoration_defs::DECOR_ID_INVALID,
                    vt: Some(Box::new(lines)),
                }),
            },
        };
        crate::marktree::marktree_put(&mut buf.b_marktree, key, -1, -1, false);

        assert_eq!(decor_find_virttext(&buf, 3, 9).unwrap().col, 7);
        assert!(decor_find_virttext(&buf, 3, 8).is_none());
    }
    use crate::buffer_defs::{BufT, WinoptT};
    use crate::decoration_defs::{DecorSignHighlight, DecorVirtText, VirtTextPos};

    #[test]
    fn hl_group_name_returns_the_numeric_id_when_names_are_disabled() {
        assert!(matches!(
            unsafe { hl_group_name(42, false) },
            crate::api::private::defs::Object::Integer(42)
        ));
        assert!(matches!(
            unsafe { hl_group_name(-1, false) },
            crate::api::private::defs::Object::Integer(-1)
        ));
    }

    #[test]
    fn hl_group_name_returns_a_string_object_when_names_are_enabled() {
        let _lock = crate::globals::global_state_test_lock();
        assert!(matches!(
            unsafe { hl_group_name(0, true) },
            crate::api::private::defs::Object::String(name) if name.is_empty()
        ));
    }

    fn range_with(data: DecorRangeData) -> DecorRange {
        DecorRange {
            start_row: 0,
            start_col: 0,
            end_row: 0,
            end_col: 0,
            ordering: 0,
            priority_internal: crate::decoration_defs::DECOR_PRIORITY_BASE,
            owned: false,
            data,
            attr_id: 0,
            draw_col: DECOR_DRAW_COL_UNDECIDED,
        }
    }

    // --- decor_put_vt / decor_virt_line_wrap ---

    #[test]
    fn decor_put_vt_links_the_new_node_ahead_of_the_existing_chain() {
        let tail = decor_put_vt(
            DecorVirtText { col: 2, ..DecorVirtText::default() },
            None,
        );
        let head = decor_put_vt(
            DecorVirtText { col: 1, ..DecorVirtText::default() },
            Some(tail),
        );
        assert_eq!(head.col, 1);
        let next = head.next.as_ref().expect("chain should continue");
        assert_eq!(next.col, 2);
        assert!(next.next.is_none());
    }

    /// The `next` argument REPLACES whatever link the value being
    /// copied happened to carry.
    #[test]
    fn decor_put_vt_overwrites_the_next_link_of_its_input() {
        let stale = decor_put_vt(
            DecorVirtText { col: 9, ..DecorVirtText::default() },
            None,
        );
        let vt = DecorVirtText { col: 1, next: Some(stale), ..DecorVirtText::default() };
        let node = decor_put_vt(vt, None);
        assert!(node.next.is_none());
    }

    /// `Wrap` always wraps and `Auto` follows the window's 'wrap',
    /// while `Trunc`/`Scroll` never do - so 'wrap' must not be
    /// consulted for them.
    #[test]
    fn virt_line_wrap_follows_the_overflow_mode_and_only_then_the_wrap_option() {
        use crate::decoration_defs::VirtLineOverflow as O;
        let mut wp = crate::buffer_defs::WinT::default();

        for wrap in [0, 1] {
            wp.w_onebuf_opt.wo_wrap = wrap;
            assert!(decor_virt_line_wrap(&wp, O::Wrap));
            assert!(!decor_virt_line_wrap(&wp, O::Trunc));
            assert!(!decor_virt_line_wrap(&wp, O::Scroll));
        }

        wp.w_onebuf_opt.wo_wrap = 0;
        assert!(!decor_virt_line_wrap(&wp, O::Auto));
        wp.w_onebuf_opt.wo_wrap = 1;
        assert!(decor_virt_line_wrap(&wp, O::Auto));
    }

    // --- may_force_numberwidth_recompute ---

    /// Installs a single-window, single-tabpage layout into the
    /// globals for the duration of a test and restores the previous
    /// pointers on drop, even through a panic - so a failing test
    /// cannot leave dangling window/tabpage pointers in the globals
    /// for whichever test runs next.
    struct SingleWindowLayoutGuard {
        prev_first_tabpage: *mut crate::buffer_defs::TabpageT,
        prev_curtab: *mut crate::buffer_defs::TabpageT,
        prev_firstwin: *mut crate::buffer_defs::WinT,
    }

    impl SingleWindowLayoutGuard {
        fn install(
            win: *mut crate::buffer_defs::WinT,
            tp: *mut crate::buffer_defs::TabpageT,
        ) -> Self {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            let me = Self {
                prev_first_tabpage: globals.first_tabpage,
                prev_curtab: globals.curtab,
                prev_firstwin: globals.firstwin,
            };
            globals.first_tabpage = tp;
            globals.curtab = tp;
            globals.firstwin = win;
            me
        }
    }

    impl Drop for SingleWindowLayoutGuard {
        fn drop(&mut self) {
            let globals = unsafe { crate::globals::GLOBALS.get_mut() };
            globals.first_tabpage = self.prev_first_tabpage;
            globals.curtab = self.prev_curtab;
            globals.firstwin = self.prev_firstwin;
        }
    }

    /// Builds a window that satisfies every condition, so each test
    /// below can break exactly one of them.
    fn numberwidth_win(buf: *mut BufT) -> Box<crate::buffer_defs::WinT> {
        let mut win = Box::new(crate::buffer_defs::WinT {
            w_minscwidth: crate::option_vars::SCL_NUM,
            w_nrwidth_width: 1,
            w_nrwidth_line_count: 42,
            ..Default::default()
        });
        win.w_buffer = buf;
        win.w_onebuf_opt.wo_nu = 1;
        win
    }

    #[test]
    fn numberwidth_recompute_is_forced_for_a_matching_window() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = Box::new(BufT::default());
        let mut win = numberwidth_win(std::ptr::from_mut(&mut *buf));
        let mut tp = Box::new(crate::buffer_defs::TabpageT::default());
        let _g = SingleWindowLayoutGuard::install(
            std::ptr::from_mut(&mut *win),
            std::ptr::from_mut(&mut *tp),
        );
        unsafe { may_force_numberwidth_recompute(std::ptr::from_mut(&mut *buf), false) };
        assert_eq!(win.w_nrwidth_line_count, 0);
    }

    /// A window showing a DIFFERENT buffer must be left alone.
    #[test]
    fn numberwidth_recompute_skips_windows_on_another_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = Box::new(BufT::default());
        let mut other = Box::new(BufT::default());
        let mut win = numberwidth_win(std::ptr::from_mut(&mut *buf));
        let mut tp = Box::new(crate::buffer_defs::TabpageT::default());
        let _g = SingleWindowLayoutGuard::install(
            std::ptr::from_mut(&mut *win),
            std::ptr::from_mut(&mut *tp),
        );
        unsafe { may_force_numberwidth_recompute(std::ptr::from_mut(&mut *other), true) };
        assert_eq!(win.w_nrwidth_line_count, 42);
    }

    /// Only 'signcolumn' == "number" shares the number column, so any
    /// other value means a sign cannot change its width.
    #[test]
    fn numberwidth_recompute_skips_windows_without_signcolumn_number() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = Box::new(BufT::default());
        let mut win = numberwidth_win(std::ptr::from_mut(&mut *buf));
        win.w_minscwidth = 1;
        let mut tp = Box::new(crate::buffer_defs::TabpageT::default());
        let _g = SingleWindowLayoutGuard::install(
            std::ptr::from_mut(&mut *win),
            std::ptr::from_mut(&mut *tp),
        );
        unsafe { may_force_numberwidth_recompute(std::ptr::from_mut(&mut *buf), true) };
        assert_eq!(win.w_nrwidth_line_count, 42);
    }

    /// With neither 'number' nor 'relativenumber' there is no number
    /// column at all; 'relativenumber' alone is enough.
    #[test]
    fn numberwidth_recompute_needs_either_number_option() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = Box::new(BufT::default());
        let mut win = numberwidth_win(std::ptr::from_mut(&mut *buf));
        win.w_onebuf_opt.wo_nu = 0;
        win.w_onebuf_opt.wo_rnu = 0;
        let mut tp = Box::new(crate::buffer_defs::TabpageT::default());
        let g = SingleWindowLayoutGuard::install(
            std::ptr::from_mut(&mut *win),
            std::ptr::from_mut(&mut *tp),
        );
        unsafe { may_force_numberwidth_recompute(std::ptr::from_mut(&mut *buf), true) };
        assert_eq!(win.w_nrwidth_line_count, 42);

        win.w_onebuf_opt.wo_rnu = 1;
        unsafe { may_force_numberwidth_recompute(std::ptr::from_mut(&mut *buf), true) };
        assert_eq!(win.w_nrwidth_line_count, 0);
        drop(g);
    }

    /// The `unplace` asymmetry: removing a sign can only shrink the
    /// column, so it always recomputes. PLACING one only matters
    /// while the column is still narrower than two cells - an
    /// implementation ignoring `unplace` would skip this window.
    #[test]
    fn numberwidth_recompute_always_runs_when_unplacing_but_not_when_placing_into_a_wide_column() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = Box::new(BufT::default());
        let mut win = numberwidth_win(std::ptr::from_mut(&mut *buf));
        win.w_nrwidth_width = 5;
        let mut tp = Box::new(crate::buffer_defs::TabpageT::default());
        let g = SingleWindowLayoutGuard::install(
            std::ptr::from_mut(&mut *win),
            std::ptr::from_mut(&mut *tp),
        );

        // Placing into an already-wide column: nothing to do.
        unsafe { may_force_numberwidth_recompute(std::ptr::from_mut(&mut *buf), false) };
        assert_eq!(win.w_nrwidth_line_count, 42);

        // Unplacing from the very same window: always recompute.
        unsafe { may_force_numberwidth_recompute(std::ptr::from_mut(&mut *buf), true) };
        assert_eq!(win.w_nrwidth_line_count, 0);
        drop(g);
    }

    // --- decor_state_invalidate / decor_redraw_end ---

    /// Installs a window pointer into `DECOR_STATE` for the duration
    /// of a test and restores the previous one on drop, even through
    /// a panic, so a failing test cannot leave a dangling pointer in
    /// the file-static for whichever test runs next.
    struct DecorStateWinGuard {
        saved_win: *mut crate::buffer_defs::WinT,
        saved_valid: bool,
    }

    impl DecorStateWinGuard {
        fn install(win: *mut crate::buffer_defs::WinT, itr_valid: bool) -> Self {
            let state = unsafe { DECOR_STATE.get_mut() };
            let me = Self { saved_win: state.win, saved_valid: state.itr_valid };
            state.win = win;
            state.itr_valid = itr_valid;
            me
        }
    }

    impl Drop for DecorStateWinGuard {
        fn drop(&mut self) {
            let state = unsafe { DECOR_STATE.get_mut() };
            state.win = self.saved_win;
            state.itr_valid = self.saved_valid;
        }
    }

    #[test]
    fn decor_state_spell_defaults_to_off_not_undecided() {
        // The original zero-initializes decor_state, and TriState's
        // zero value is False. TriState::default() is None (-1),
        // which would mean "undecided" instead.
        assert_eq!(DecorState::default().spell, TriState::False);
    }

    #[test]
    fn decor_state_invalidate_ignores_a_state_with_no_window() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = Box::new(BufT::default());
        let _guard = DecorStateWinGuard::install(std::ptr::null_mut(), true);
        unsafe { decor_state_invalidate(std::ptr::from_mut(&mut *buf)) };
        assert!(unsafe { DECOR_STATE.get_mut() }.itr_valid);
    }

    #[test]
    fn decor_state_invalidate_clears_the_iterator_for_its_own_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = Box::new(BufT::default());
        let mut win = Box::new(crate::buffer_defs::WinT::default());
        win.w_buffer = std::ptr::from_mut(&mut *buf);
        let _guard = DecorStateWinGuard::install(std::ptr::from_mut(&mut *win), true);
        unsafe { decor_state_invalidate(std::ptr::from_mut(&mut *buf)) };
        assert!(!unsafe { DECOR_STATE.get_mut() }.itr_valid);
    }

    /// A change to some OTHER buffer cannot disturb this window's
    /// iterator. An implementation that invalidated unconditionally
    /// would needlessly discard it here.
    #[test]
    fn decor_state_invalidate_leaves_the_iterator_alone_for_another_buffer() {
        let _lock = crate::globals::global_state_test_lock();
        let mut own_buf = Box::new(BufT::default());
        let mut other_buf = Box::new(BufT::default());
        let mut win = Box::new(crate::buffer_defs::WinT::default());
        win.w_buffer = std::ptr::from_mut(&mut *own_buf);
        let _guard = DecorStateWinGuard::install(std::ptr::from_mut(&mut *win), true);
        unsafe { decor_state_invalidate(std::ptr::from_mut(&mut *other_buf)) };
        assert!(unsafe { DECOR_STATE.get_mut() }.itr_valid);
    }

    #[test]
    fn decor_redraw_end_detaches_the_state_from_its_window() {
        let mut win = Box::new(crate::buffer_defs::WinT::default());
        let mut state = DecorState { win: std::ptr::from_mut(&mut *win), ..DecorState::default() };
        unsafe { decor_redraw_end(&mut state) };
        assert!(state.win.is_null());
    }

    #[test]
    fn decor_range_slot_models_the_freelist_chain() {
        // A freed slot carries the index of the next free slot; the
        // end of the chain is the sentinel, not slot 0.
        let slots = [
            DecorRangeSlot::Free(2),
            DecorRangeSlot::Range(range_with(DecorRangeData::VirtText(std::ptr::null_mut()))),
            DecorRangeSlot::Free(DECOR_NO_FREE_SLOT),
        ];
        let mut i = 0;
        let mut visited = Vec::new();
        loop {
            match slots[i as usize] {
                DecorRangeSlot::Free(next) => {
                    visited.push(i);
                    if next == DECOR_NO_FREE_SLOT {
                        break;
                    }
                    i = next;
                }
                DecorRangeSlot::Range(_) => panic!("walked into an occupied slot"),
            }
        }
        assert_eq!(visited, vec![0, 2]);
    }

    // --- sign_item_cmp ---

    fn sign_item(priority: u16, id: u32, sign_add_id: i32) -> crate::sign_defs::SignItem {
        crate::sign_defs::SignItem {
            sh: Some(Box::new(DecorSignHighlight {
                priority,
                sign_add_id,
                ..DecorSignHighlight::default()
            })),
            id,
        }
    }

    /// Priority is the FIRST key and sorts descending: the
    /// higher-priority sign must be placed first. An ascending
    /// comparator returns the opposite sign here.
    #[test]
    fn sign_item_cmp_puts_the_higher_priority_sign_first() {
        let high = sign_item(20, 1, 1);
        let low = sign_item(10, 1, 1);
        assert!(sign_item_cmp(&high, &low) < 0);
        assert!(sign_item_cmp(&low, &high) > 0);
    }

    /// Id only breaks a priority tie, and also descending.
    #[test]
    fn sign_item_cmp_breaks_a_priority_tie_on_the_higher_id() {
        let newer = sign_item(10, 2, 1);
        let older = sign_item(10, 1, 1);
        assert!(sign_item_cmp(&newer, &older) < 0);
        assert!(sign_item_cmp(&older, &newer) > 0);
    }

    /// A lower priority must lose even when its id is higher, proving
    /// the keys are applied in order rather than combined.
    #[test]
    fn sign_item_cmp_prefers_priority_over_id() {
        let high_prio_low_id = sign_item(20, 1, 1);
        let low_prio_high_id = sign_item(10, 99, 1);
        assert!(sign_item_cmp(&high_prio_low_id, &low_prio_high_id) < 0);
    }

    #[test]
    fn sign_item_cmp_breaks_a_full_tie_on_the_higher_sign_add_id() {
        let newer = sign_item(10, 1, 5);
        let older = sign_item(10, 1, 2);
        assert!(sign_item_cmp(&newer, &older) < 0);
        assert!(sign_item_cmp(&older, &newer) > 0);
    }

    #[test]
    fn sign_item_cmp_is_zero_only_when_all_three_keys_match() {
        assert_eq!(sign_item_cmp(&sign_item(10, 1, 1), &sign_item(10, 1, 1)), 0);
    }

    /// Sorting a real list with this comparator must place the signs
    /// in the order the sign column actually draws them.
    #[test]
    fn sign_item_cmp_sorts_a_list_highest_priority_first() {
        let mut items = [
            sign_item(10, 1, 1),
            sign_item(30, 2, 1),
            sign_item(20, 3, 1),
        ];
        items.sort_by(|a, b| sign_item_cmp(a, b).cmp(&0));
        let priorities: Vec<u16> =
            items.iter().map(|i| i.sh.as_ref().unwrap().priority).collect();
        assert_eq!(priorities, vec![30, 20, 10]);
    }

    #[test]
    fn decor_range_kind_is_derived_from_its_payload() {
        assert_eq!(
            range_with(DecorRangeData::Highlight(DecorSignHighlight::default())).kind(),
            DecorRangeKind::Highlight
        );
        assert_eq!(
            range_with(DecorRangeData::Sign(DecorSignHighlight::default())).kind(),
            DecorRangeKind::Sign
        );
        assert_eq!(
            range_with(DecorRangeData::VirtText(std::ptr::null_mut())).kind(),
            DecorRangeKind::VirtText
        );
        assert_eq!(
            range_with(DecorRangeData::VirtLines(std::ptr::null_mut())).kind(),
            DecorRangeKind::VirtLines
        );
        assert_eq!(
            range_with(DecorRangeData::UIWatched {
                ns_id: 1,
                mark_id: 2,
                pos: VirtTextPos::Inline,
            })
            .kind(),
            DecorRangeKind::UIWatched
        );
    }

    /// Highlight and Sign share one union member in the original, as
    /// do VirtText and VirtLines; the derived tag must still tell each
    /// pair apart.
    #[test]
    fn decor_range_kind_separates_the_payload_sharing_pairs() {
        assert_ne!(
            range_with(DecorRangeData::Highlight(DecorSignHighlight::default())).kind(),
            range_with(DecorRangeData::Sign(DecorSignHighlight::default())).kind()
        );
        assert_ne!(
            range_with(DecorRangeData::VirtText(std::ptr::null_mut())).kind(),
            range_with(DecorRangeData::VirtLines(std::ptr::null_mut())).kind()
        );
    }

    #[test]
    fn only_virt_text_and_ui_watched_have_a_virtual_position() {
        assert!(decor_virt_pos(&range_with(DecorRangeData::VirtText(
            std::ptr::null_mut()
        ))));
        assert!(decor_virt_pos(&range_with(DecorRangeData::UIWatched {
            ns_id: 0,
            mark_id: 0,
            pos: VirtTextPos::EndOfLine,
        })));
        // Virtual LINES carry the same payload type as virtual text
        // but are not virtually positioned.
        assert!(!decor_virt_pos(&range_with(DecorRangeData::VirtLines(
            std::ptr::null_mut()
        ))));
        assert!(!decor_virt_pos(&range_with(DecorRangeData::Highlight(
            DecorSignHighlight::default()
        ))));
        assert!(!decor_virt_pos(&range_with(DecorRangeData::Sign(
            DecorSignHighlight::default()
        ))));
    }

    #[test]
    fn virt_pos_kind_reads_the_position_out_of_virtual_text() {
        let mut vt = Box::new(DecorVirtText {
            pos: VirtTextPos::RightAlign,
            ..DecorVirtText::default()
        });
        let range = range_with(DecorRangeData::VirtText(std::ptr::from_mut(&mut *vt)));
        assert_eq!(
            unsafe { decor_virt_pos_kind(&range) },
            VirtTextPos::RightAlign
        );
    }

    #[test]
    fn virt_pos_kind_reads_the_position_out_of_a_ui_watched_mark() {
        let range = range_with(DecorRangeData::UIWatched {
            ns_id: 7,
            mark_id: 9,
            pos: VirtTextPos::WinCol,
        });
        assert_eq!(unsafe { decor_virt_pos_kind(&range) }, VirtTextPos::WinCol);
    }

    /// The original tests `kind == kDecorKindVirtText` specifically,
    /// so virtual LINES fall through to the unused end-of-line
    /// fallback even though their payload has a perfectly readable
    /// `pos`. An implementation that read `pos` whenever a
    /// `DecorVirtText` was present would return `Overlay` here.
    #[test]
    fn virt_pos_kind_does_not_read_the_position_out_of_virtual_lines() {
        let mut vt = Box::new(DecorVirtText {
            pos: VirtTextPos::Overlay,
            ..DecorVirtText::virt_lines_init()
        });
        let range = range_with(DecorRangeData::VirtLines(std::ptr::from_mut(&mut *vt)));
        assert_eq!(
            unsafe { decor_virt_pos_kind(&range) },
            VirtTextPos::EndOfLine
        );
    }

    #[test]
    fn virt_pos_kind_falls_back_for_decorations_with_no_virtual_position() {
        for data in [
            DecorRangeData::Highlight(DecorSignHighlight::default()),
            DecorRangeData::Sign(DecorSignHighlight::default()),
        ] {
            assert_eq!(
                unsafe { decor_virt_pos_kind(&range_with(data)) },
                VirtTextPos::EndOfLine
            );
        }
    }

    /// The default buffer has `autom == false`, so the guard's very
    /// first operand already ends the call - this is the shape every
    /// currently-reachable call takes.
    #[test]
    fn signcols_count_range_returns_early_without_auto_signcolumn() {
        let mut buf = BufT::default();
        assert!(!buf.b_signcols.autom);
        buf_signcols_count_range(&mut buf, 0, 5, 1, TriState::False);
        assert_eq!(buf.b_signcols.max, 0);
        assert_eq!(buf.b_signcols.count[0], 0);
    }

    /// With `autom` forced on, an inverted range still returns before
    /// the marktree is consulted.
    #[test]
    fn signcols_count_range_returns_early_for_an_inverted_range() {
        let mut buf = BufT::default();
        buf.b_signcols.autom = true;
        buf_signcols_count_range(&mut buf, 7, 3, 1, TriState::None);
        assert_eq!(buf.b_signcols.max, 0);
    }

    /// The operand that stays true for real sessions until an extmark
    /// API exists: a buffer with no sign-text mark at all.
    #[test]
    fn signcols_count_range_returns_early_without_any_sign_text_mark() {
        let mut buf = BufT::default();
        buf.b_signcols.autom = true;
        assert_eq!(buf_meta_total(&buf, MetaIndex::SignText), 0);
        buf_signcols_count_range(&mut buf, 0, 0, -1, TriState::True);
        assert_eq!(buf.b_signcols.max, 0);
    }

    fn win_with_cole(cole: crate::types_defs::OptInt, buf: *mut BufT) -> WinT {
        WinT { w_onebuf_opt: WinoptT { wo_cole: cole, ..Default::default() }, w_buffer: buf, ..Default::default() }
    }

    #[test]
    fn decor_conceal_line_false_by_default_conceallevel() {
        let mut buf = BufT::default();
        let wp = win_with_cole(0, &mut buf as *mut BufT);
        assert!(!unsafe { decor_conceal_line(&wp, 0, false) });
    }

    #[test]
    fn decor_conceal_line_false_for_negative_row_regardless_of_conceallevel() {
        let mut buf = BufT::default();
        let wp = win_with_cole(0, &mut buf as *mut BufT);
        assert!(!unsafe { decor_conceal_line(&wp, -1, false) });
    }

    #[test]
    #[should_panic(expected = "not yet translated")]
    fn decor_conceal_line_panics_when_conceallevel_is_2_or_higher() {
        // Not achievable via any real translated function yet (nothing
        // can raise 'conceallevel') - pokes it directly to prove the
        // real, faithfully-translated short-circuit condition is in
        // place, independent of how wo_cole eventually gets set.
        let mut buf = BufT::default();
        let wp = win_with_cole(2, &mut buf as *mut BufT);
        let _ = unsafe { decor_conceal_line(&wp, 0, false) };
    }

    #[test]
    fn decor_virt_lines_zero_when_no_virt_lines_meta() {
        let mut buf = BufT::default();
        let wp = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        assert_eq!(unsafe { decor_virt_lines(&wp, 0, 1, None, None, true) }, 0);
    }

    #[test]
    #[should_panic(expected = "not yet translated")]
    fn decor_virt_lines_panics_when_meta_total_is_nonzero() {
        // Not achievable via any real translated function yet (nothing
        // can attach virtual lines) - pokes the marktree meta_root
        // directly to prove the real, faithfully-translated check is
        // in place, independent of how it eventually gets populated.
        let mut buf = BufT::default();
        buf.b_marktree.meta_root[MetaIndex::Lines as usize] = 1;
        let wp = WinT { w_buffer: &mut buf as *mut BufT, ..Default::default() };
        let _ = unsafe { decor_virt_lines(&wp, 0, 1, None, None, true) };
    }

    #[test]
    fn win_lines_concealed_false_by_default() {
        let mut buf = BufT::default();
        let wp = win_with_cole(0, &mut buf as *mut BufT);
        assert!(!unsafe { win_lines_concealed(&wp) });
    }

    #[test]
    fn win_lines_concealed_true_when_conceallevel_is_2_or_higher() {
        let mut buf = BufT::default();
        let wp = win_with_cole(2, &mut buf as *mut BufT);
        assert!(unsafe { win_lines_concealed(&wp) });
    }

    #[test]
    fn win_lines_concealed_true_when_folding_may_exist_even_with_conceallevel_0() {
        let mut buf = BufT::default();
        let wp = WinT {
            // 'foldenable' on, 'foldmethod' unset (NOT "manual" by
            // default) - has_any_folding's own "no folds" fast path
            // only applies when foldmethod IS manual with no real
            // folds, so this genuinely reports true.
            w_onebuf_opt: WinoptT { wo_fen: 1, ..Default::default() },
            w_buffer: &mut buf as *mut BufT,
            ..Default::default()
        };
        assert!(unsafe { win_lines_concealed(&wp) });
    }
}
