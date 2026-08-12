//! Translated from `src/nvim/syntax.c` (tractable core only).
//!
//! `syntax.c` (~7500 lines) is neovim's syntax-highlighting engine:
//! the `:syntax` command family, the pattern/keyword/cluster tables,
//! and the per-line state machine that drives highlighting. Almost
//! every function depends on `synstate_T`/`stateitem_T`/`synpat_T`
//! and the regex engine. The common `regprog_T` header and
//! `reg_extmatch_T` captures are translated, but the compiler/executor
//! that produces them is not.
//!
//! Translated: [`limit_pos`] and [`syn_compare_stub`] - two small,
//! self-contained helpers with no design freedom of their own,
//! needing only the already-real [`crate::pos_defs::LposT`]. Both are
//! translated ahead of their real callers (`syn_add_end_off`/
//! `syn_add_start_off` and the cluster-list sort respectively, none
//! translated), matching this crate's established "translate a small,
//! mechanically-correct piece ahead of the surrounding engine"
//! precedent (e.g. `drawline.rs`'s `get_lcs_ext`).
//! [`syn_set_timeout`] also translates the syntax engine's timeout
//! pointer setter independently of the recognition engine.
//! [`set_context_in_echohl_cmd`] configures command-line completion
//! for `:echohl`/`:match`.
//!
//! Deferred: everything else in the file.

use crate::pos_defs::LposT;

/// Syntax-recognition timeout (`syn_tm`); null means no timeout.
static SYN_TM: crate::globals::GlobalCell<*mut crate::types_defs::ProftimeT> =
    crate::globals::GlobalCell::new(std::ptr::null_mut());
/// Include `"None"` in highlight-name completion (`include_none`).
static INCLUDE_NONE: crate::globals::GlobalCell<i32> = crate::globals::GlobalCell::new(0);

/// Set or clear the syntax-recognition timeout (`syn_set_timeout`).
///
/// # Safety
/// Must not run concurrently with syntax recognition or another
/// timeout update.
pub unsafe fn syn_set_timeout(tm: *mut crate::types_defs::ProftimeT) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *SYN_TM.get_mut() = tm };
}

/// Configure completion for `:match`/`:echohl`
/// (`set_context_in_echohl_cmd`).
///
/// # Safety
/// Must not run concurrently with another syntax-completion update.
pub unsafe fn set_context_in_echohl_cmd(
    xp: &mut crate::cmdexpand_defs::ExpandT,
    arg: &[u8],
) {
    xp.xp_context = crate::cmdexpand_defs::ExpandContext::Highlight;
    xp.xp_pattern = Some(arg.to_vec());
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *INCLUDE_NONE.get_mut() = 1 };
}

/// `syn_buf` - the buffer the syntax engine is currently highlighting.
///
/// Only ever set by `syn_update`/`syntax_start` (not translated), so
/// this stays null in this crate today - the same treatment already
/// given to `insexpand`'s own completion statics.
static SYN_BUF: crate::globals::GlobalCell<*mut crate::buffer_defs::BufT> =
    crate::globals::GlobalCell::new(std::ptr::null_mut());

/// `current_lnum` - the line the syntax engine is currently on.
static CURRENT_LNUM: crate::globals::GlobalCell<crate::pos_defs::LinenrT> =
    crate::globals::GlobalCell::new(0);

/// `current_flags` - the `HL_*` flags of the current character.
static CURRENT_FLAGS: crate::globals::GlobalCell<i32> = crate::globals::GlobalCell::new(0);

/// `current_seqnr` - the sequence number of the current syntax item,
/// used to identify a concealed region.
static CURRENT_SEQNR: crate::globals::GlobalCell<i32> = crate::globals::GlobalCell::new(0);

/// `current_sub_char` - the conceal substitution character in force at
/// the current position.
static CURRENT_SUB_CHAR: crate::globals::GlobalCell<i32> = crate::globals::GlobalCell::new(0);

/// `current_state` - the stack of syntax items in force at the current
/// position, innermost last.
///
/// The original is a `garray_T` whose `ga_itemsize` doubles as a
/// validity sentinel for `invalidate_current_state`. A `Vec` has no
/// counterpart for that, so the flag will be added explicitly once the
/// functions that read it are translated and its uses are visible; for
/// now this is just the stack itself.
static CURRENT_STATE: crate::globals::GlobalCell<Vec<StateItemT>> =
    crate::globals::GlobalCell::new(Vec::new());

/// Deep-clear the current syntax stack (`clear_current_state`).
///
/// # Safety
/// Every non-null `si_extmatch` must hold one live reference, and no
/// other syntax-state operation may run concurrently.
#[allow(dead_code)]
unsafe fn clear_current_state() {
    // SAFETY: forwarded from this function's own safety doc.
    let state = unsafe { CURRENT_STATE.get_mut() };
    for item in state.drain(..) {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { crate::regexp::unref_extmatch(item.si_extmatch) };
    }
}

/// `current_col` - the column the syntax engine is currently on.
static CURRENT_COL: crate::globals::GlobalCell<crate::pos_defs::ColnrT> =
    crate::globals::GlobalCell::new(0);

/// `keepend_level` - the stack level of the first item with `keepend`,
/// or -1 when there is none.
///
/// Checking for `keepend` is expensive, so [`check_keepend`] uses this
/// to skip the work entirely below the level where one exists.
static KEEPEND_LEVEL: crate::globals::GlobalCell<i32> = crate::globals::GlobalCell::new(-1);

/// The syntax flags and sequence number at the current position
/// (`get_syntax_info`).
///
/// The original writes the sequence number through a `seqnrp`
/// out-parameter and returns the flags; both come back as a tuple
/// here, matching this crate's convention for out-parameters.
///
/// # Safety
/// Reads the `CURRENT_FLAGS`/`CURRENT_SEQNR` file-statics.
#[must_use]
pub unsafe fn get_syntax_info() -> (i32, i32) {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*CURRENT_FLAGS.get_mut(), *CURRENT_SEQNR.get_mut()) }
}

/// The conceal substitution character at the current position
/// (`syn_get_sub_char`).
///
/// # Safety
/// Reads the `CURRENT_SUB_CHAR` file-static.
#[must_use]
pub unsafe fn syn_get_sub_char() -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { *CURRENT_SUB_CHAR.get_mut() }
}

/// The fold level implied by the current syntax state
/// (`syn_cur_foldlevel`).
///
/// One level is contributed by each item on the state stack that was
/// defined with `fold`, so nested folded regions nest their levels.
///
/// # Safety
/// Reads the `CURRENT_STATE` file-static.
#[must_use]
pub unsafe fn syn_cur_foldlevel() -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let state = unsafe { &*CURRENT_STATE.get_mut() };
    state
        .iter()
        .filter(|si| si.si_flags & hl_flags::FOLD != 0)
        .count() as i32
}

/// Pushes a cleared state onto the current state stack, with its
/// pattern index set to `idx` (`push_current_state`).
///
/// # Safety
/// Mutates the `CURRENT_STATE` file-static.
pub unsafe fn push_current_state(idx: i32) {
    // SAFETY: forwarded from this function's own safety doc.
    let state = unsafe { &mut *CURRENT_STATE.get_mut() };
    state.push(StateItemT {
        si_idx: idx,
        ..Default::default()
    });
}

/// The number of items on the current state stack
/// (`current_state.ga_len`).
///
/// # Safety
/// Reads the `CURRENT_STATE` file-static.
#[must_use]
pub unsafe fn current_state_len() -> usize {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*CURRENT_STATE.get_mut()).len() }
}

/// Propagates a `keepend` match-end down to contained items, up to
/// the first `skipend` item (`check_keepend`).
///
/// Walking the whole stack is expensive, so the scan starts at the
/// level where a `keepend` actually exists. Within that range the last
/// `extend` item wins: `keepend` items below it have no effect, since
/// `extend` explicitly ignores a `keepend`.
///
/// # Safety
/// Reads `KEEPEND_LEVEL` and mutates the `CURRENT_STATE` file-static.
pub unsafe fn check_keepend() {
    // SAFETY: forwarded from this function's own safety doc.
    let keepend_level = unsafe { *KEEPEND_LEVEL.get_mut() };
    if keepend_level < 0 {
        return;
    }
    // SAFETY: as above.
    let state = unsafe { &mut *CURRENT_STATE.get_mut() };

    // Find the last "extend" item; without one the scan covers every
    // item from `keepend_level` upwards.
    let mut i = state.len() as i32 - 1;
    while i > keepend_level {
        if state[i as usize].si_flags & hl_flags::EXTEND != 0 {
            break;
        }
        i -= 1;
    }

    let mut maxpos = LposT::default();
    let mut maxpos_h = LposT::default();
    // `i` only stays below zero when the stack is empty, which cannot
    // happen here: `pop_current_state` resets `keepend_level` to -1
    // once it reaches the stack length, and that case returned above.
    for sip in state[i.max(0) as usize..].iter_mut() {
        if maxpos.lnum != 0 {
            limit_pos_zero(&mut sip.si_m_endpos, &maxpos);
            limit_pos_zero(&mut sip.si_h_endpos, &maxpos_h);
            limit_pos_zero(&mut sip.si_eoe_pos, &maxpos);
            sip.si_ends = true;
        }
        if sip.si_ends && (sip.si_flags & hl_flags::KEEPEND) != 0 {
            if maxpos.lnum == 0
                || maxpos.lnum > sip.si_m_endpos.lnum
                || (maxpos.lnum == sip.si_m_endpos.lnum && maxpos.col > sip.si_m_endpos.col)
            {
                maxpos = sip.si_m_endpos;
            }
            if maxpos_h.lnum == 0
                || maxpos_h.lnum > sip.si_h_endpos.lnum
                || (maxpos_h.lnum == sip.si_h_endpos.lnum && maxpos_h.col > sip.si_h_endpos.col)
            {
                maxpos_h = sip.si_h_endpos;
            }
        }
    }
}

/// Whether pattern `idx` was already matched at the current position
/// (`did_match_already`).
///
/// `zero_width` holds the indices of zero-width matches that carried a
/// `nextgroup` argument: those are never pushed onto the state stack,
/// and can only match once anyway, so they are tracked separately.
/// The original passes that list as a `garray_T` of `int`; it is a
/// plain slice here, since nothing in this function grows it.
///
/// # Safety
/// Reads the `CURRENT_STATE`/`CURRENT_COL`/`CURRENT_LNUM` file-statics.
#[must_use]
pub unsafe fn did_match_already(idx: i32, zero_width: &[i32]) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let (state, col, lnum) = unsafe {
        (
            &*CURRENT_STATE.get_mut(),
            *CURRENT_COL.get_mut(),
            *CURRENT_LNUM.get_mut(),
        )
    };

    if state
        .iter()
        .any(|si| si.si_m_startcol == col && si.si_m_lnum == lnum && si.si_idx == idx)
    {
        return true;
    }

    zero_width.contains(&idx)
}

/// The text of the current line in the syntax buffer
/// (`syn_getcurline`).
///
/// Note this reads `syn_buf`, NOT `curbuf`: highlighting can run over
/// a buffer other than the one being edited.
///
/// # Safety
/// `SYN_BUF` must be a valid, non-null pointer to a live `BufT` - the
/// original dereferences it without checking. Forwarded from
/// [`crate::memline::ml_get_buf`]'s own safety doc.
#[must_use]
pub unsafe fn syn_getcurline() -> Vec<u8> {
    // SAFETY: forwarded from this function's own safety doc.
    let buf = unsafe { *SYN_BUF.get_mut() };
    // SAFETY: forwarded from this function's own safety doc.
    let lnum = unsafe { *CURRENT_LNUM.get_mut() };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::memline::ml_get_buf(&mut *buf, lnum) }
}

/// The length of the current line in the syntax buffer
/// (`syn_getcurline_len`).
///
/// # Safety
/// Same as [`syn_getcurline`].
#[must_use]
pub unsafe fn syn_getcurline_len() -> crate::pos_defs::ColnrT {
    // SAFETY: forwarded from this function's own safety doc.
    let buf = unsafe { *SYN_BUF.get_mut() };
    // SAFETY: forwarded from this function's own safety doc.
    let lnum = unsafe { *CURRENT_LNUM.get_mut() };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::memline::ml_get_buf_len(&mut *buf, lnum) }
}

/// Syntax group ID for `contains=TOP` (`SYNID_TOP`).
pub const SYNID_TOP: i32 = 21000;
/// Syntax group ID for `contains=CONTAINED` (`SYNID_CONTAINED`).
pub const SYNID_CONTAINED: i32 = 22000;
/// First syntax group ID for clusters (`SYNID_CLUSTER`).
pub const SYNID_CLUSTER: i32 = 23000;
/// Maximum number of clusters before the group ID overflows
/// (`MAX_CLUSTER_ID`).
pub const MAX_CLUSTER_ID: i32 = 32767 - SYNID_CLUSTER;

/// Per-syntax-item flags (the `HL_*` group in `syntax.h`).
///
/// These are OR'd together into a single `int`, so they are plain
/// constants in a module rather than an enum - matching the
/// `opt_flags`/`xp_bs` precedent already used in this crate for the
/// original's other bit-flag groups.
pub mod hl_flags {
    /// not used on toplevel (`HL_CONTAINED`).
    pub const CONTAINED: i32 = 0x01;
    /// has no highlighting (`HL_TRANSP`).
    pub const TRANSP: i32 = 0x02;
    /// match within one line only (`HL_ONELINE`).
    pub const ONELINE: i32 = 0x04;
    /// end pattern that matches with `$` (`HL_HAS_EOL`).
    pub const HAS_EOL: i32 = 0x08;
    /// sync point after this item, syncing only (`HL_SYNC_HERE`).
    pub const SYNC_HERE: i32 = 0x10;
    /// sync point at current line, syncing only (`HL_SYNC_THERE`).
    pub const SYNC_THERE: i32 = 0x20;
    /// use match ID instead of item ID (`HL_MATCH`).
    pub const MATCH: i32 = 0x40;
    /// nextgroup can skip newlines (`HL_SKIPNL`).
    pub const SKIPNL: i32 = 0x80;
    /// nextgroup can skip white space (`HL_SKIPWHITE`).
    pub const SKIPWHITE: i32 = 0x100;
    /// nextgroup can skip empty lines (`HL_SKIPEMPTY`).
    pub const SKIPEMPTY: i32 = 0x200;
    /// end match always kept (`HL_KEEPEND`).
    pub const KEEPEND: i32 = 0x400;
    /// exclude NL from match (`HL_EXCLUDENL`).
    pub const EXCLUDENL: i32 = 0x800;
    /// only used for displaying, not syncing (`HL_DISPLAY`).
    pub const DISPLAY: i32 = 0x1000;
    /// define fold (`HL_FOLD`).
    pub const FOLD: i32 = 0x2000;
    /// ignore a keepend (`HL_EXTEND`).
    pub const EXTEND: i32 = 0x4000;
    /// match continued from previous line (`HL_MATCHCONT`).
    pub const MATCHCONT: i32 = 0x8000;
    /// transparent item without contains arg (`HL_TRANS_CONT`).
    pub const TRANS_CONT: i32 = 0x10000;
    /// can be concealed (`HL_CONCEAL`).
    pub const CONCEAL: i32 = 0x20000;
    /// can be concealed (`HL_CONCEALENDS`).
    pub const CONCEALENDS: i32 = 0x40000;
    /// toplevel item in included syntax, allowed by `contains=TOP`
    /// (`HL_INCLUDED_TOPLEVEL`).
    pub const INCLUDED_TOPLEVEL: i32 = 0x80000;
}

/// Adjust a syntax item declared in a `:syn include`d file
/// (`syn_incl_toplevel`).
///
/// Sets the contained flag, and if the item is not already contained,
/// adds it to the top-level group when there is one.
///
/// The original allocates a two-element, zero-terminated list to hand
/// to `syn_combine_list` (which then frees it); a one-element `Vec`
/// carries the same content, since this crate's group-id lists are
/// length-bounded rather than zero-terminated.
///
/// # Safety
/// `GLOBALS.curwin` must be valid and non-null, as must its `w_s`
/// syntax block.
pub unsafe fn syn_incl_toplevel(id: i32, flagsp: &mut i32) {
    // SAFETY: forwarded from this function's own safety doc.
    let block = unsafe { &mut *(*crate::globals::GLOBALS.get_mut().curwin).w_s };

    if (*flagsp & hl_flags::CONTAINED) != 0 || block.b_syn_topgrp == 0 {
        return;
    }
    *flagsp |= hl_flags::CONTAINED | hl_flags::INCLUDED_TOPLEVEL;

    if block.b_syn_topgrp >= SYNID_CLUSTER {
        let tlg_id = (block.b_syn_topgrp - SYNID_CLUSTER) as usize;
        let mut list = std::mem::take(&mut block.b_syn_clusters.items[tlg_id].scl_list);
        syn_combine_list(&mut list, vec![id as i16], ClusterOp::Add);
        block.b_syn_clusters.items[tlg_id].scl_list = list;
    }
}

/// Find a syntax cluster by name and return its group ID, or `0` when
/// there is none (`syn_scl_name2id`).
///
/// The comparison is case-INSENSITIVE, done by uppercasing the needle
/// once and matching it against each cluster's precomputed
/// `scl_name_u` - the original notes this avoids repeated `stricmp`
/// calls, which are slow on some systems.
///
/// The scan runs BACKWARDS, so with duplicate names the LAST entry
/// wins.
///
/// # Safety
/// `GLOBALS.curwin` must be valid and non-null, as must its `w_s`
/// syntax block.
#[must_use]
pub unsafe fn syn_scl_name2id(name: &[u8]) -> i32 {
    let name_u = crate::strings::vim_strsave_up(name);
    // SAFETY: forwarded from this function's own safety doc.
    let block = unsafe { &*(*crate::globals::GLOBALS.get_mut().curwin).w_s };

    let mut i = block.b_syn_clusters.ga_len() - 1;
    while i >= 0 {
        let matched = block
            .b_syn_clusters
            .get(i)
            .and_then(|c| c.scl_name_u.as_deref())
            .is_some_and(|u| u == name_u.as_slice());
        if matched {
            break;
        }
        i -= 1;
    }
    if i < 0 { 0 } else { i + SYNID_CLUSTER }
}

/// Like [`syn_scl_name2id`], but takes the name as a slice that may
/// run on past its end (`syn_scl_namen2id`).
///
/// The original copies out `len` bytes first; a subslice does the
/// same without allocating.
///
/// # Safety
/// Same as [`syn_scl_name2id`].
#[must_use]
pub unsafe fn syn_scl_namen2id(linep: &[u8], len: usize) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { syn_scl_name2id(&linep[..len.min(linep.len())]) }
}

/// Find a syntax cluster by name, creating it when it doesn't exist
/// yet, and return its group ID (`syn_check_cluster`).
///
/// Returns `0` on failure. `pp` may run on past the name, so `len`
/// bounds it; the original copies those bytes out first, while a
/// subslice does the same without allocating.
///
/// # Safety
/// Same as [`syn_scl_name2id`].
pub unsafe fn syn_check_cluster(pp: &[u8], len: usize) -> i32 {
    // Matches the original's own `xstrnsave`: the stored name is
    // NUL-terminated, like every other string in the cluster table.
    let name = crate::strings::xstrnsave(pp, len.min(pp.len()));
    // SAFETY: forwarded from this function's own safety doc.
    let id = unsafe { syn_scl_name2id(&name) };
    if id == 0 {
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { syn_add_cluster(name) }
    } else {
        id
    }
}

/// Append a new syntax cluster and return its group ID
/// (`syn_add_cluster`).
///
/// Returns `0` when the cluster table is already full. The original's
/// `E848` message is omitted (message display is not translated),
/// keeping the same `0` return and the same untouched table.
///
/// The original's first-call growarray init (`ga_itemsize`/
/// `ga_set_growsize`) has no counterpart here: `TypedGarrayT` owns a
/// real `Vec<SynClusterT>`, so there is no item size to record, and
/// growth is the `Vec`'s own business.
///
/// # Safety
/// Same as [`syn_scl_name2id`].
pub unsafe fn syn_add_cluster(name: Vec<u8>) -> i32 {
    // SAFETY: forwarded from this function's own safety doc.
    let block = unsafe { &mut *(*crate::globals::GLOBALS.get_mut().curwin).w_s };

    let len = block.b_syn_clusters.ga_len();
    if len >= MAX_CLUSTER_ID {
        return 0;
    }

    let name_u = crate::strings::vim_strsave_up(&name);
    // The spell cluster ids are recorded before `name` is moved into
    // the table.
    if crate::strings::vim_stricmp(&name, b"Spell") == 0 {
        block.b_spell_cluster_id = len + SYNID_CLUSTER;
    }
    if crate::strings::vim_stricmp(&name, b"NoSpell") == 0 {
        block.b_nospell_cluster_id = len + SYNID_CLUSTER;
    }

    block.b_syn_clusters.items.push(SynClusterT {
        scl_name: Some(name),
        scl_name_u: Some(name_u),
        scl_list: Vec::new(),
    });

    len + SYNID_CLUSTER
}

/// What [`syn_combine_list`] should do with the second list
/// (`CLUSTER_REPLACE`/`CLUSTER_ADD`/`CLUSTER_SUBTRACT`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClusterOp {
    /// replace the first list with the second (`CLUSTER_REPLACE`).
    Replace,
    /// add the second list to the first (`CLUSTER_ADD`).
    Add,
    /// subtract the second list from the first (`CLUSTER_SUBTRACT`).
    Subtract,
}

/// Combine two syntax group-id lists in place (`syn_combine_list`).
///
/// `clstr2` is consumed, matching the original: it is either moved
/// into `clstr1` or freed, and the caller must not use it again. A
/// by-value `Vec` says exactly that, where the original's
/// `int16_t **` leaves the caller holding a dangling pointer.
///
/// The original's empty lists are represented by a NULL pointer, never
/// by a list holding only its terminator (an empty merge result is
/// stored as NULL), so an empty `Vec` is the faithful equivalent of
/// both.
///
/// Both lists are sorted, then merged in one pass. The original walks
/// them twice - once to count, once to fill a freshly sized
/// allocation - which a growable `Vec` makes unnecessary.
pub fn syn_combine_list(clstr1: &mut Vec<i16>, clstr2: Vec<i16>, list_op: ClusterOp) {
    // Handle degenerate cases.
    if clstr2.is_empty() {
        return;
    }
    if clstr1.is_empty() || list_op == ClusterOp::Replace {
        if matches!(list_op, ClusterOp::Replace | ClusterOp::Add) {
            *clstr1 = clstr2;
        }
        // Subtracting from an empty list leaves it empty, and drops
        // `clstr2` - the original's `xfree` of the same.
        return;
    }

    // For speed purposes, sort both lists.
    let mut g1 = std::mem::take(clstr1);
    let mut g2 = clstr2;
    g1.sort_by(|a, b| syn_compare_stub(*a, *b).cmp(&0));
    g2.sort_by(|a, b| syn_compare_stub(*a, *b).cmp(&0));

    // Merge, always taking from the first list, and from the second
    // only when adding.
    let mut out: Vec<i16> = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    while i < g1.len() && j < g2.len() {
        // We always want to add from the first list.
        if g1[i] < g2[j] {
            out.push(g1[i]);
            i += 1;
            continue;
        }
        // We only want to add from the second list if we're adding
        // the lists.
        if list_op == ClusterOp::Add {
            out.push(g2[j]);
        }
        // An id present in both is consumed from the first too, which
        // is what makes a subtract remove it.
        if g1[i] == g2[j] {
            i += 1;
        }
        j += 1;
    }

    // Now add the leftovers from whichever list didn't get finished
    // first. As before, only take from the second when adding.
    out.extend_from_slice(&g1[i..]);
    if list_op == ClusterOp::Add {
        out.extend_from_slice(&g2[j..]);
    }

    *clstr1 = out;
}

/// One syntax cluster - a named set of syntax group ids
/// (`syn_cluster_T`).
///
/// All three fields are owned by the original (each is `xfree`d in
/// `syn_clear_cluster`), so they become owned Rust values.
/// `scl_list` is a plain `Vec` rather than the original's
/// separately-counted `int16_t *`, since a `Vec` carries its own
/// length.
///
/// Both name fields are NUL-terminated, matching the original: it
/// builds `scl_name` with `xstrnsave` and `scl_name_u` with
/// `vim_strsave_up`, and this crate's equivalents of both append a
/// NUL. Anything comparing against these must account for it - the
/// lookups do so by uppercasing the needle through the same
/// `vim_strsave_up`, so both sides carry a NUL and match.
#[derive(Debug, Clone, Default)]
pub struct SynClusterT {
    /// syntax cluster name, NUL-terminated (`scl_name`).
    pub scl_name: Option<Vec<u8>>,
    /// uppercase of `scl_name`, NUL-terminated (`scl_name_u`).
    pub scl_name_u: Option<Vec<u8>>,
    /// IDs in this syntax cluster (`scl_list`).
    pub scl_list: Vec<i16>,
}

/// Release everything one syntax cluster owns
/// (`syn_clear_cluster`).
///
/// The original's three `xfree`s become plain resets: dropping the
/// owned values is what frees them. Note the ENTRY itself is left in
/// place - the original frees only its members, leaving the slot for
/// the caller to remove or reuse.
pub fn syn_clear_cluster(cluster: &mut SynClusterT) {
    cluster.scl_name = None;
    cluster.scl_name_u = None;
    cluster.scl_list = Vec::new();
}

/// Split a `:syntax` argument into its group NAME and the rest
/// (`get_group_name`).
///
/// Returns `(name_end, rest)`, both byte offsets into `arg`:
/// `name_end` is just past the group name, and `rest` is the first
/// argument after it. `None` means there were not enough arguments.
///
/// The original writes `name_end` through an out-parameter and
/// returns `rest`; returning both is this crate's convention. Note
/// `name_end` is still meaningful to the caller when this returns
/// `None`, but the original leaves it written in that case too, so
/// nothing is lost by only returning it on success - no caller reads
/// it after a failure.
///
/// The emptiness test deliberately checks for a NUL rather than for
/// the end of a command: the first argument may be a pattern, in
/// which case `|` is a legitimate part of it and must not terminate
/// the scan.
#[must_use]
pub fn get_group_name(arg: &[u8]) -> Option<(usize, usize)> {
    let name_end = crate::charset::skiptowhite(arg);
    let rest = name_end + crate::charset::skipwhite(&arg[name_end.min(arg.len())..]);

    // Check if there are enough arguments. The first argument may be
    // a pattern, where '|' is allowed, so only check for NUL.
    let first = arg.first().copied().unwrap_or(0);
    let at_rest = arg.get(rest).copied().unwrap_or(0);
    if crate::ex_docmd::ends_excmd(first) || at_rest == 0 {
        return None;
    }
    Some((name_end, rest))
}

/// Clamp `pos` so it does not run past `limit` (`limit_pos`).
///
/// A position on a LATER line is pulled back to `limit` entirely -
/// both line and column - while a position on the SAME line only has
/// its column clamped. A position on an earlier line is left alone,
/// even if its column is greater, since a column only orders
/// positions within one line.
pub fn limit_pos(pos: &mut LposT, limit: &LposT) {
    if pos.lnum > limit.lnum {
        *pos = *limit;
    } else if pos.lnum == limit.lnum && pos.col > limit.col {
        pos.col = limit.col;
    }
}

/// Number of entries in a syntax block's `in_id_list` result cache
/// (`IDL_CACHE_SIZE`).
pub const IDL_CACHE_SIZE: usize = 16;

/// One cached `in_id_list` resolution (`idl_entry_T`).
///
/// `idl_key` is the list this entry resolves. It is NON-OWNING: the
/// original compares it by POINTER identity against the caller's list
/// and never frees it, so it stays a raw pointer here rather than
/// becoming an owned `Vec`. Only `idl_ids` is owned.
///
/// The original's separate `idl_count` has no counterpart - `idl_ids`
/// is a `Vec` and carries its own length.
#[derive(Debug, Default, Clone)]
pub struct IdlEntryT {
    /// the list this entry resolves, null when unused (`idl_key`).
    pub idl_key: *const i16,
    /// sorted, cluster-expanded group IDs (`idl_ids`).
    pub idl_ids: Vec<i16>,
    /// leading `ALLBUT`/`TOP`/`CONTAINED` item, or `0`
    /// (`idl_marker`).
    pub idl_marker: i16,
    /// `false` means not cacheable, so the slow path must be used
    /// (`idl_usable`).
    pub idl_usable: bool,
}

/// A syntax block's `in_id_list` result cache (`idl_cache_T`).
#[derive(Debug)]
pub struct IdlCacheT {
    /// the cached entries (`ent`).
    pub ent: [IdlEntryT; IDL_CACHE_SIZE],
    /// round-robin replacement index (`round`).
    pub round: i32,
    /// index of the last returned entry, the fast path (`last`).
    pub last: i32,
}

impl Default for IdlCacheT {
    /// An empty cache. Written out rather than derived because
    /// `[T; 16]` only derives `Default` for `T: Copy`, and
    /// [`IdlEntryT`] owns a `Vec`.
    fn default() -> Self {
        IdlCacheT {
            ent: std::array::from_fn(|_| IdlEntryT::default()),
            round: 0,
            last: 0,
        }
    }
}

/// Release a syntax block's `in_id_list` cache (`idl_cache_clear`).
///
/// The original frees each entry's `idl_ids` and then the cache
/// itself; reclaiming the `Box` does both, since every owned field
/// drops with it. `idl_key` is deliberately not freed - it never was,
/// being a borrowed pointer to the caller's list.
///
/// Does nothing when no cache has been built yet.
///
/// # Safety
/// `block.b_idlist_cache`, when non-null, must have been allocated
/// with `Box::into_raw` - this reclaims it with `Box::from_raw`.
pub unsafe fn idl_cache_clear(block: &mut crate::buffer_defs::SynblockT) {
    if block.b_idlist_cache.is_null() {
        return;
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe {
        drop(Box::from_raw(block.b_idlist_cache));
    }
    block.b_idlist_cache = std::ptr::null_mut();
}

/// Prepare the syntax pattern table for its first use
/// (`init_syn_patterns`).
///
/// The original also records `sizeof(synpat_T)` in `ga_itemsize`;
/// that has no counterpart, since [`crate::garray_defs::TypedGarrayT`]
/// carries the item type itself.
///
/// Note this sets the grow size WITHOUT clearing the table, so
/// `ga_growsize` is assigned directly rather than going through
/// `ga_init`, which would also empty it - the original calls
/// `ga_set_growsize`, not `ga_init`.
///
/// # Safety
/// `GLOBALS.curwin` must be valid and non-null, as must its `w_s`
/// syntax block.
pub unsafe fn init_syn_patterns() {
    // SAFETY: forwarded from this function's own safety doc.
    let block = unsafe { &mut *(*crate::globals::GLOBALS.get_mut().curwin).w_s };
    block.b_syn_patterns.ga_growsize = 10;
}

/// Comparator ordering two syntax group IDs ascending (`idl_id_cmp`).
///
/// Returns a negative/zero/positive `i32`, matching `qsort`'s own
/// convention and this crate's established comparator shape. The
/// subtraction is done in `i32`, as the original's casts make explicit,
/// so two far-apart `i16` values cannot overflow the result.
#[must_use]
pub fn idl_id_cmp(a: i16, b: i16) -> i32 {
    i32::from(a) - i32::from(b)
}

/// Whether `id` is in the resolved set of cache entry `e`
/// (`idl_contains`).
///
/// `idl_ids` is kept sorted ascending by [`idl_id_cmp`], so this is a
/// binary search. The original hand-rolls one over a pointer/count
/// pair; `idl_ids` is a `Vec` here, so the standard library's own
/// binary search does the same job.
#[must_use]
pub fn idl_contains(e: &IdlEntryT, id: i16) -> bool {
    e.idl_ids.binary_search(&id).is_ok()
}

/// Reset the highlight-group completion counters
/// (`reset_expand_highlight`).
///
/// Clears all three, so a following completion offers only the real
/// groups until something opts the keywords back in - see
/// [`crate::highlight_group::get_highlight_name_ext`], which reads
/// them.
///
/// # Safety
/// Touches `GLOBALS`' `include_*` counters.
pub unsafe fn reset_expand_highlight() {
    // SAFETY: forwarded from this function's own safety doc.
    let g = unsafe { crate::globals::GLOBALS.get_mut() };
    g.include_link = 0;
    g.include_default = 0;
    g.include_none = 0;
}

/// Walk `off` characters from byte index `col` in `line`, forwards for
/// a positive `off` and backwards for a negative one, and return the
/// resulting byte index.
///
/// Shared by [`syn_add_start_off`] and [`syn_add_end_off`], which the
/// original spells out twice. Walking stops at the line's NUL going
/// forwards and at its start going backwards, so the result is always
/// a valid index. Movement is per CHARACTER, not per byte.
fn walk_off(line: &[u8], col: usize, off: i32) -> usize {
    let mut p = col.min(line.len());
    if off > 0 {
        let mut n = off;
        while n > 0 && line.get(p).copied().unwrap_or(0) != 0 {
            // SAFETY: `utfc_ptr2len` only touches `OPTION_VARS`.
            p += (unsafe { crate::mbyte::utfc_ptr2len(&line[p..]) }).max(1) as usize;
            p = p.min(line.len());
            n -= 1;
        }
    } else if off < 0 {
        let mut n = off;
        while n < 0 && p > 0 {
            // SAFETY: `utf_head_off` only reads the slice it is given.
            let back = unsafe { crate::mbyte::utf_head_off(line, p - 1) };
            p -= back as usize + 1;
            n += 1;
        }
    }
    p
}

/// Compute the END position of a match, applying pattern offset `idx`
/// (`syn_add_end_off`).
///
/// The offset is measured from the match's START when the low flag bit
/// for `idx` is set (and then `extra` is added), otherwise from its
/// END. `extra` exists for the `matchgroup` case.
///
/// Note the difference from [`syn_add_start_off`]: when the resulting
/// line runs past the end of the buffer this sets the column to `0`
/// and SKIPS the offset walk entirely, because the original's second
/// test is an `else if`. It also leaves the line number past the end
/// rather than clamping it.
///
/// # Safety
/// `SYN_BUF` must be a valid, non-null pointer to a live `BufT`.
pub unsafe fn syn_add_end_off(
    result: &mut LposT,
    regmatch: &crate::regexp_defs::RegmmatchT,
    spp: &SynpatT,
    idx: usize,
    extra: i32,
) {
    let (lnum, mut col, off) = if spp.sp_off_flags & (1 << idx) != 0 {
        (
            regmatch.startpos[0].lnum,
            regmatch.startpos[0].col,
            spp.sp_offsets[idx] + extra,
        )
    } else {
        (regmatch.endpos[0].lnum, regmatch.endpos[0].col, spp.sp_offsets[idx])
    };
    result.lnum = lnum;

    // SAFETY: forwarded from this function's own safety doc.
    let buf = unsafe { *SYN_BUF.get_mut() };
    // SAFETY: forwarded from this function's own safety doc.
    let line_count = unsafe { &*buf }.b_ml.ml_line_count;

    // Don't go past the end of the line. Matters for "rs=e+2" when
    // there is a matchgroup. Watch out for a match with the last NL in
    // the buffer.
    if result.lnum > line_count {
        col = 0;
    } else if off != 0 {
        // SAFETY: forwarded from this function's own safety doc.
        let line = unsafe { crate::memline::ml_get_buf(&mut *buf, result.lnum) };
        col = walk_off(&line, col as usize, off) as crate::pos_defs::ColnrT;
    }
    result.col = col;
}

/// Compute the START position of a match, applying pattern offset
/// `idx` (`syn_add_start_off`).
///
/// The offset is measured from the match's END when the HIGH flag bit
/// for `idx` is set - the original shifts by `SPO_COUNT`, so each
/// offset has a separate "from which end" bit - otherwise from its
/// start.
///
/// Unlike [`syn_add_end_off`], a line past the end of the buffer is
/// CLAMPED to the last line (a `\n` at the end of the pattern can take
/// the match below it), the column set to that line's length, and the
/// offset walk still runs.
///
/// # Safety
/// Same as [`syn_add_end_off`].
pub unsafe fn syn_add_start_off(
    result: &mut LposT,
    regmatch: &crate::regexp_defs::RegmmatchT,
    spp: &SynpatT,
    idx: usize,
    extra: i32,
) {
    let (lnum, mut col, off) = if spp.sp_off_flags & (1 << (idx + spo::COUNT)) != 0 {
        (
            regmatch.endpos[0].lnum,
            regmatch.endpos[0].col,
            spp.sp_offsets[idx] + extra,
        )
    } else {
        (
            regmatch.startpos[0].lnum,
            regmatch.startpos[0].col,
            spp.sp_offsets[idx],
        )
    };
    result.lnum = lnum;

    // SAFETY: forwarded from this function's own safety doc.
    let buf = unsafe { *SYN_BUF.get_mut() };
    // SAFETY: forwarded from this function's own safety doc.
    let line_count = unsafe { &*buf }.b_ml.ml_line_count;

    if result.lnum > line_count {
        // A "\n" at the end of the pattern may take us below the last
        // line.
        result.lnum = line_count;
        // SAFETY: forwarded from this function's own safety doc.
        col = unsafe { crate::memline::ml_get_buf_len(&mut *buf, result.lnum) };
    }
    if off != 0 {
        // SAFETY: forwarded from this function's own safety doc.
        let line = unsafe { crate::memline::ml_get_buf(&mut *buf, result.lnum) };
        col = walk_off(&line, col as usize, off) as crate::pos_defs::ColnrT;
    }
    result.col = col;
}

/// Pattern-offset slots in [`SynpatT::sp_offsets`] (`SPO_*`).
pub mod spo {
    /// match start offset (`SPO_MS_OFF`).
    pub const MS_OFF: usize = 0;
    /// match end offset (`SPO_ME_OFF`).
    pub const ME_OFF: usize = 1;
    /// highlight start offset (`SPO_HS_OFF`).
    pub const HS_OFF: usize = 2;
    /// highlight end offset (`SPO_HE_OFF`).
    pub const HE_OFF: usize = 3;
    /// region start offset (`SPO_RS_OFF`).
    pub const RS_OFF: usize = 4;
    /// region end offset (`SPO_RE_OFF`).
    pub const RE_OFF: usize = 5;
    /// leading context offset (`SPO_LC_OFF`).
    pub const LC_OFF: usize = 6;
    /// number of offset slots (`SPO_COUNT`).
    pub const COUNT: usize = 7;
}

/// What kind of item a [`SynpatT`] is (`SPTYPE_*`).
pub mod sptype {
    /// match keyword with this group ID (`SPTYPE_MATCH`).
    pub const MATCH: u8 = 1;
    /// match a regexp, start of item (`SPTYPE_START`).
    pub const START: u8 = 2;
    /// match a regexp, end of item (`SPTYPE_END`).
    pub const END: u8 = 3;
    /// match a regexp, skip within item (`SPTYPE_SKIP`).
    pub const SKIP: u8 = 4;
}

/// The syntax-group identity of one item, as passed to `in_id_list`
/// (`struct sp_syn`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpSyn {
    /// `":syn include"` unique tag (`inc_tag`).
    pub inc_tag: i32,
    /// highlight group ID of the item (`id`).
    pub id: i16,
    /// `contained in` group IDs (`cont_in_list`).
    ///
    /// A `Vec` rather than the original's zero-terminated
    /// `int16_t *`, matching [`SynClusterT::scl_list`].
    pub cont_in_list: Vec<i16>,
}

/// One syntax pattern item (`synpat_T`).
///
/// A start/skip/end item consists of n start patterns, one skip
/// pattern and m end patterns; for the latter two the patterns are
/// always consecutive (start-skip-end).
///
/// The original's three owned pointers become owned Rust values:
/// `sp_pattern` a `Vec<u8>` and both group-ID lists `Vec<i16>`,
/// matching [`SynClusterT`]. `sp_prog` stays a raw pointer to the
/// still-opaque `RegprogT`, exactly as [`crate::regexp_defs::RegmmatchT`]
/// does - nothing translated dereferences it.
///
/// The original notes its field ORDER is chosen to reduce padding.
/// That is a C layout concern with no observable behaviour, so the
/// fields are grouped for readability here instead.
#[derive(Debug, Default)]
pub struct SynpatT {
    /// see [`sptype`] (`sp_type`).
    pub sp_type: u8,
    /// this item is used for syncing (`sp_syncing`).
    pub sp_syncing: bool,
    /// highlight group ID of the pattern (`sp_syn_match_id`).
    pub sp_syn_match_id: i16,
    /// which offsets are set, and from which end (`sp_off_flags`).
    pub sp_off_flags: i16,
    /// the offsets themselves, indexed by [`spo`] (`sp_offsets`).
    pub sp_offsets: [i32; spo::COUNT],
    /// see [`hl_flags`] (`sp_flags`).
    pub sp_flags: i32,
    /// conceal substitute character (`sp_cchar`).
    pub sp_cchar: i32,
    /// ignore-case flag for `sp_prog` (`sp_ic`).
    pub sp_ic: i32,
    /// sync item index, syncing only (`sp_sync_idx`).
    pub sp_sync_idx: i32,
    /// ID of the last line where this was tried (`sp_line_id`).
    pub sp_line_id: i32,
    /// next match in the `sp_line_id` line (`sp_startcol`).
    pub sp_startcol: i32,
    /// `contains` group IDs (`sp_cont_list`).
    pub sp_cont_list: Vec<i16>,
    /// `nextgroup` group IDs (`sp_next_list`).
    pub sp_next_list: Vec<i16>,
    /// the item's own syntax identity (`sp_syn`).
    pub sp_syn: SpSyn,
    /// the regexp to match, as a pattern (`sp_pattern`).
    pub sp_pattern: Vec<u8>,
    /// the regexp to match, as a compiled program (`sp_prog`).
    pub sp_prog: *mut crate::types_defs::RegprogT,
    /// timing for this pattern (`sp_time`).
    pub sp_time: crate::buffer_defs::SynTimeT,
}

/// The value of [`StateItemT::si_idx`] for keywords (`KEYWORD_IDX`).
///
/// Every other value indexes the syntax-pattern array, so callers
/// distinguish the two with a plain sign test rather than a
/// comparison against this constant.
pub const KEYWORD_IDX: i32 = -1;

/// One entry of the current syntax state stack (`stateitem_T`).
///
/// For the current state more than just the pattern index has to be
/// remembered. When `si_m_endpos.lnum` is 0 every field other than
/// `si_idx` is unknown; the end positions hold the column of the
/// character *after* the match.
///
/// As with [`SynpatT`], the two owned group-ID lists become `Vec<i16>`
/// instead of the original's zero-terminated `int16_t *`, while
/// `si_extmatch` stays a raw pointer because `reg_extmatch_T` is
/// reference-counted - the same split [`SynpatT::sp_prog`] makes.
#[derive(Debug, Default)]
pub struct StateItemT {
    /// index of the syntax pattern, or [`KEYWORD_IDX`] (`si_idx`).
    pub si_idx: i32,
    /// highlight group ID for keywords (`si_id`).
    pub si_id: i32,
    /// same as `si_id`, with transparency removed (`si_trans_id`).
    pub si_trans_id: i32,
    /// line number of the match (`si_m_lnum`).
    pub si_m_lnum: i32,
    /// starting column of the match (`si_m_startcol`).
    pub si_m_startcol: i32,
    /// just after the end position of the match (`si_m_endpos`).
    pub si_m_endpos: LposT,
    /// start position of the highlighting (`si_h_startpos`).
    pub si_h_startpos: LposT,
    /// end position of the highlighting (`si_h_endpos`).
    pub si_h_endpos: LposT,
    /// end position of the end pattern (`si_eoe_pos`).
    pub si_eoe_pos: LposT,
    /// group ID for the end pattern, or zero (`si_end_idx`).
    pub si_end_idx: i32,
    /// whether the match ends before `si_m_endpos` (`si_ends`).
    ///
    /// An `int` in the original, but only ever assigned `true`/`false`
    /// and tested as a condition, so it is a `bool` here - as
    /// [`SynpatT::sp_syncing`] already is.
    pub si_ends: bool,
    /// attributes in this state (`si_attr`).
    pub si_attr: i32,
    /// `HL_HAS_EOL` for this state, plus `HL_SKIP*` for `si_next_list`
    /// (`si_flags`). See [`hl_flags`].
    pub si_flags: i32,
    /// sequence number (`si_seqnr`).
    pub si_seqnr: i32,
    /// substitution character for conceal (`si_cchar`).
    pub si_cchar: i32,
    /// list of contained groups (`si_cont_list`).
    ///
    /// **Incomplete**: the original's `int16_t *` carries three
    /// states: none (`NULL`); "everything not contained"
    /// (`ID_LIST_ALL`, the sentinel `(int16_t *)-1` set by
    /// `update_si_attr` for a transparent item that isn't inside
    /// anything); and a real list. A `Vec` cannot express the middle
    /// one, and it is also the wrong ownership: the original only ever
    /// *borrows* here, either from a pattern's `sp_cont_list`, a
    /// cluster's `scl_list`, or the parent state item, whose pointer
    /// it copies directly. That is why `pop_current_state` frees
    /// `si_extmatch` but never this.
    ///
    /// Picking the final shape is deliberately deferred, because it is
    /// tied to the `in_id_list` cache: `IdlEntryT::idl_key` keys on
    /// the list's *pointer identity*, so a representation that copies
    /// the ids would silently break cache lookups. Decide it together
    /// with `in_id_list`/`idl_get_entry`, where every consumer is
    /// visible.
    pub si_cont_list: Vec<i16>,
    /// `nextgroup` IDs to use after this item ends (`si_next_list`).
    pub si_next_list: Vec<i16>,
    /// `\z(...\)` matches from the start pattern (`si_extmatch`).
    pub si_extmatch: *mut crate::types_defs::RegExtmatchT,
}

/// Whether any syntax items are defined for the syntax block `block`
/// (`syntax_present`).
///
/// The original takes a window and reaches through `w_s`; this takes
/// the syntax block directly, since that is all it reads and the
/// callers already have one in hand.
pub fn syntax_present(block: &crate::buffer_defs::SynblockT) -> bool {
    block.b_syn_patterns.ga_len() != 0
        || block.b_syn_clusters.ga_len() != 0
        || block.b_keywtab.ht_used > 0
        || block.b_keywtab_ic.ht_used > 0
}

/// Clear the syntax timing for the current buffer (`syntime_clear`).
///
/// Does nothing when no syntax is active. The original's
/// "No Syntax items defined for this buffer" message is omitted,
/// matching this crate's established policy of skipping message
/// display while keeping identical state and control flow.
///
/// # Safety
/// `GLOBALS.curwin` must be valid and non-null, as must its `w_s`
/// syntax block.
pub unsafe fn syntime_clear() {
    // SAFETY: forwarded from this function's own safety doc.
    let block = unsafe { &mut *(*crate::globals::GLOBALS.get_mut().curwin).w_s };
    if !syntax_present(block) {
        return;
    }
    for spp in &mut block.b_syn_patterns.items {
        syn_clear_time(&mut spp.sp_time);
    }
}

/// Reset one syntax item's timing counters (`syn_clear_time`).
pub fn syn_clear_time(st: &mut crate::buffer_defs::SynTimeT) {
    st.total = crate::profile::profile_zero();
    st.slowest = crate::profile::profile_zero();
    st.count = 0;
    st.match_ = 0;
}

/// Clamp `pos` so it does not run past `limit`, treating a zero line
/// number as "unset" (`limit_pos_zero`).
///
/// An unset position adopts `limit` wholesale rather than being
/// clamped, since line 0 would otherwise always compare as earlier
/// than `limit` and so be left alone by [`limit_pos`].
pub fn limit_pos_zero(pos: &mut LposT, limit: &LposT) {
    if pos.lnum == 0 {
        *pos = *limit;
    } else {
        limit_pos(pos, limit);
    }
}

/// Comparator ordering two syntax cluster ids ascending
/// (`syn_compare_stub`).
///
/// Returns a negative/zero/positive `i32`, matching `qsort`'s own
/// convention and this crate's established comparator shape.
#[must_use]
pub fn syn_compare_stub(s1: i16, s2: i16) -> i32 {
    if s1 > s2 {
        1
    } else if s1 < s2 {
        -1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct SynTimeoutGuard(*mut crate::types_defs::ProftimeT);

    impl SynTimeoutGuard {
        fn capture() -> Self {
            Self(unsafe { *SYN_TM.get_mut() })
        }
    }

    struct IncludeNoneGuard(i32);

    impl IncludeNoneGuard {
        fn capture() -> Self {
            Self(unsafe { *INCLUDE_NONE.get_mut() })
        }
    }

    impl Drop for IncludeNoneGuard {
        fn drop(&mut self) {
            unsafe { *INCLUDE_NONE.get_mut() = self.0 };
        }
    }

    impl Drop for SynTimeoutGuard {
        fn drop(&mut self) {
            unsafe { *SYN_TM.get_mut() = self.0 };
        }
    }

    #[test]
    fn syn_set_timeout_installs_and_clears_the_timeout_pointer() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = SynTimeoutGuard::capture();
        let mut timeout = 123;
        let timeout_ptr = std::ptr::addr_of_mut!(timeout);

        unsafe { syn_set_timeout(timeout_ptr) };
        assert_eq!(unsafe { *SYN_TM.get_mut() }, timeout_ptr);

        unsafe { syn_set_timeout(std::ptr::null_mut()) };
        assert!(unsafe { *SYN_TM.get_mut() }.is_null());
    }

    #[test]
    fn set_context_in_echohl_cmd_selects_highlight_completion_and_none() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = IncludeNoneGuard::capture();
        unsafe { *INCLUDE_NONE.get_mut() = 0 };
        let mut xp = crate::cmdexpand_defs::ExpandT::default();

        unsafe { set_context_in_echohl_cmd(&mut xp, b"Err") };

        assert_eq!(
            xp.xp_context,
            crate::cmdexpand_defs::ExpandContext::Highlight
        );
        assert_eq!(xp.xp_pattern.as_deref(), Some(b"Err".as_slice()));
        assert_eq!(unsafe { *INCLUDE_NONE.get_mut() }, 1);
    }


    // ---- syn_getcurline / syn_getcurline_len ----

    /// Restores `SYN_BUF`/`CURRENT_LNUM` on drop, even through a
    /// panic, so a failing test cannot leave a dangling pointer in
    /// the global for whichever test runs next.
    struct SynBufGuard {
        buf: *mut crate::buffer_defs::BufT,
        lnum: crate::pos_defs::LinenrT,
    }

    impl SynBufGuard {
        fn install(buf: *mut crate::buffer_defs::BufT, lnum: crate::pos_defs::LinenrT) -> Self {
            let me = Self {
                buf: unsafe { *SYN_BUF.get_mut() },
                lnum: unsafe { *CURRENT_LNUM.get_mut() },
            };
            unsafe { *SYN_BUF.get_mut() = buf };
            unsafe { *CURRENT_LNUM.get_mut() = lnum };
            me
        }
    }

    impl Drop for SynBufGuard {
        fn drop(&mut self) {
            unsafe { *SYN_BUF.get_mut() = self.buf };
            unsafe { *CURRENT_LNUM.get_mut() = self.lnum };
        }
    }

    fn syntax_test_buf(line: &[u8]) -> crate::buffer_defs::BufT {
        let mut buf = crate::buffer_defs::BufT::default();
        assert_eq!(unsafe { crate::memline::ml_open(&mut buf) }, crate::vim_defs::OK);
        assert_eq!(
            unsafe { crate::memline::ml_replace_buf_len(&mut buf, 1, line) },
            crate::vim_defs::OK
        );
        buf
    }

    fn close_syntax_test_buf(buf: crate::buffer_defs::BufT) {
        unsafe {
            let mfp = Box::from_raw(buf.b_ml.ml_mfp);
            crate::memfile::mf_close(*mfp, false);
        }
    }

    /// These read `syn_buf`, NOT `curbuf` - highlighting can run over
    /// a buffer other than the one being edited. The test points
    /// `curbuf` at a DIFFERENT buffer so an implementation reading it
    /// would return the wrong line.
    #[test]
    fn syn_getcurline_reads_the_syntax_buffer_not_the_current_one() {
        let _lock = crate::globals::global_state_test_lock();
        let mut syn = syntax_test_buf(b"syntax line\0");
        let mut other = syntax_test_buf(b"a different line\0");

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let prev_curbuf = g.curbuf;
        g.curbuf = std::ptr::from_mut(&mut other);
        let _guard = SynBufGuard::install(std::ptr::from_mut(&mut syn), 1);

        assert_eq!(unsafe { syn_getcurline() }, b"syntax line\0");

        unsafe { crate::globals::GLOBALS.get_mut() }.curbuf = prev_curbuf;
        close_syntax_test_buf(syn);
        close_syntax_test_buf(other);
    }

    /// The length excludes the terminator, unlike the text accessor
    /// which carries it.
    #[test]
    fn syn_getcurline_len_reports_the_text_length() {
        let _lock = crate::globals::global_state_test_lock();
        let mut syn = syntax_test_buf(b"abcde\0");
        let _guard = SynBufGuard::install(std::ptr::from_mut(&mut syn), 1);

        assert_eq!(unsafe { syn_getcurline_len() }, 5);

        close_syntax_test_buf(syn);
    }

    #[test]
    fn syn_getcurline_handles_an_empty_line() {
        let _lock = crate::globals::global_state_test_lock();
        let mut syn = syntax_test_buf(b"\0");
        let _guard = SynBufGuard::install(std::ptr::from_mut(&mut syn), 1);

        assert_eq!(unsafe { syn_getcurline_len() }, 0);

        close_syntax_test_buf(syn);
    }

    #[test]
    fn clear_current_state_releases_extmatches_and_empties_the_stack() {
        let _lock = crate::globals::global_state_test_lock();
        let ext = Box::into_raw(Box::new(crate::types_defs::RegExtmatchT {
            refcnt: 1,
            matches: std::array::from_fn(|index| {
                (index == 1).then(|| b"capture".to_vec())
            }),
        }));
        let _stack = CurrentStackGuard::install(vec![StateItemT {
            si_extmatch: ext,
            ..Default::default()
        }]);

        unsafe { clear_current_state() };
        assert!(unsafe { CURRENT_STATE.get_mut() }.is_empty());
    }

    // ---- get_syntax_info / syn_get_sub_char ----

    /// Restores the syntax engine's current-position statics on drop.
    struct CurrentStateGuard {
        flags: i32,
        seqnr: i32,
        sub_char: i32,
    }

    impl CurrentStateGuard {
        fn save() -> Self {
            unsafe {
                Self {
                    flags: *CURRENT_FLAGS.get_mut(),
                    seqnr: *CURRENT_SEQNR.get_mut(),
                    sub_char: *CURRENT_SUB_CHAR.get_mut(),
                }
            }
        }
    }

    impl Drop for CurrentStateGuard {
        fn drop(&mut self) {
            unsafe {
                *CURRENT_FLAGS.get_mut() = self.flags;
                *CURRENT_SEQNR.get_mut() = self.seqnr;
                *CURRENT_SUB_CHAR.get_mut() = self.sub_char;
            }
        }
    }

    /// Restores the syntax state stack on drop.
    ///
    /// Separate from [`CurrentStateGuard`] because the stack is a
    /// `Vec` rather than a `Copy` scalar, so it has to be moved out on
    /// save and moved back on drop.
    struct CurrentStackGuard {
        saved: Vec<StateItemT>,
    }

    impl CurrentStackGuard {
        fn install(stack: Vec<StateItemT>) -> Self {
            // SAFETY: the global state test lock is held by the caller.
            let slot = unsafe { &mut *CURRENT_STATE.get_mut() };
            Self {
                saved: std::mem::replace(slot, stack),
            }
        }
    }

    impl Drop for CurrentStackGuard {
        fn drop(&mut self) {
            // SAFETY: as in `install`.
            let slot = unsafe { &mut *CURRENT_STATE.get_mut() };
            *slot = std::mem::take(&mut self.saved);
        }
    }

    /// Builds a state stack whose items carry exactly `flags`.
    fn stack_with_flags(flags: &[i32]) -> Vec<StateItemT> {
        flags
            .iter()
            .map(|&si_flags| StateItemT {
                si_flags,
                ..Default::default()
            })
            .collect()
    }

    /// The flags come back as the return value and the sequence number
    /// as the second tuple element, matching the original's
    /// return-plus-out-parameter split.
    #[test]
    fn get_syntax_info_reports_flags_and_seqnr_separately() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = CurrentStateGuard::save();

        unsafe {
            *CURRENT_FLAGS.get_mut() = hl_flags::CONCEAL;
            *CURRENT_SEQNR.get_mut() = 42;
        }
        assert_eq!(unsafe { get_syntax_info() }, (hl_flags::CONCEAL, 42));

        // The two are independent: changing one must not move the
        // other.
        unsafe { *CURRENT_SEQNR.get_mut() = 7 };
        assert_eq!(unsafe { get_syntax_info() }, (hl_flags::CONCEAL, 7));
    }

    #[test]
    fn syn_get_sub_char_reports_the_conceal_substitute() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = CurrentStateGuard::save();

        unsafe { *CURRENT_SUB_CHAR.get_mut() = 0 };
        assert_eq!(unsafe { syn_get_sub_char() }, 0, "NUL means none");

        unsafe { *CURRENT_SUB_CHAR.get_mut() = i32::from(b'x') };
        assert_eq!(unsafe { syn_get_sub_char() }, i32::from(b'x'));
    }

    // ---- did_match_already ----

    /// Installs a current column/line, restoring both on drop.
    struct CurrentPosGuard(crate::pos_defs::ColnrT, crate::pos_defs::LinenrT);

    impl CurrentPosGuard {
        fn install(col: crate::pos_defs::ColnrT, lnum: crate::pos_defs::LinenrT) -> Self {
            // SAFETY: the global state test lock is held by the caller.
            unsafe {
                let saved = Self(*CURRENT_COL.get_mut(), *CURRENT_LNUM.get_mut());
                *CURRENT_COL.get_mut() = col;
                *CURRENT_LNUM.get_mut() = lnum;
                saved
            }
        }
    }

    impl Drop for CurrentPosGuard {
        fn drop(&mut self) {
            // SAFETY: as in `install`.
            unsafe {
                *CURRENT_COL.get_mut() = self.0;
                *CURRENT_LNUM.get_mut() = self.1;
            }
        }
    }

    fn matched_at(idx: i32, lnum: i32, col: i32) -> StateItemT {
        StateItemT {
            si_idx: idx,
            si_m_lnum: lnum,
            si_m_startcol: col,
            ..Default::default()
        }
    }

    #[test]
    fn did_match_already_is_false_with_nothing_matched() {
        let _lock = crate::globals::global_state_test_lock();
        let _p = CurrentPosGuard::install(4, 2);
        let _g = CurrentStackGuard::install(Vec::new());

        assert!(!unsafe { did_match_already(1, &[]) });
    }

    #[test]
    fn did_match_already_finds_a_match_at_the_current_position() {
        let _lock = crate::globals::global_state_test_lock();
        let _p = CurrentPosGuard::install(4, 2);
        let _g = CurrentStackGuard::install(vec![matched_at(1, 2, 4)]);

        assert!(unsafe { did_match_already(1, &[]) });
    }

    /// All three of pattern index, line and column must agree - a
    /// match of the same pattern elsewhere does not count.
    #[test]
    fn did_match_already_requires_the_index_line_and_column_to_all_match() {
        let _lock = crate::globals::global_state_test_lock();
        let _p = CurrentPosGuard::install(4, 2);
        let _g = CurrentStackGuard::install(vec![
            matched_at(1, 2, 5), // right pattern and line, wrong column
            matched_at(1, 3, 4), // right pattern and column, wrong line
            matched_at(9, 2, 4), // right position, wrong pattern
        ]);

        assert!(!unsafe { did_match_already(1, &[]) });
    }

    /// Zero-width matches carrying a nextgroup never reach the state
    /// stack, so they are found only via the separate list.
    #[test]
    fn did_match_already_also_checks_the_zero_width_list() {
        let _lock = crate::globals::global_state_test_lock();
        let _p = CurrentPosGuard::install(4, 2);
        let _g = CurrentStackGuard::install(Vec::new());

        assert!(unsafe { did_match_already(7, &[3, 7]) });
        assert!(!unsafe { did_match_already(5, &[3, 7]) });
    }

    // ---- check_keepend ----

    /// Installs a `keepend_level`, restoring it on drop.
    struct KeependLevelGuard(i32);

    impl KeependLevelGuard {
        fn install(level: i32) -> Self {
            // SAFETY: the global state test lock is held by the caller.
            unsafe {
                let saved = *KEEPEND_LEVEL.get_mut();
                *KEEPEND_LEVEL.get_mut() = level;
                Self(saved)
            }
        }
    }

    impl Drop for KeependLevelGuard {
        fn drop(&mut self) {
            // SAFETY: as in `install`.
            unsafe { *KEEPEND_LEVEL.get_mut() = self.0 };
        }
    }

    fn ends_at(flags: i32, lnum: i32, col: i32) -> StateItemT {
        let pos = LposT { lnum, col };
        StateItemT {
            si_flags: flags,
            si_ends: true,
            si_m_endpos: pos,
            si_h_endpos: pos,
            si_eoe_pos: pos,
            ..Default::default()
        }
    }

    /// A negative level means no item has `keepend`, so the whole
    /// (expensive) scan is skipped and nothing is touched.
    #[test]
    fn check_keepend_does_nothing_without_a_keepend_level() {
        let _lock = crate::globals::global_state_test_lock();
        let _k = KeependLevelGuard::install(-1);
        let _g = CurrentStackGuard::install(vec![
            ends_at(hl_flags::KEEPEND, 1, 5),
            ends_at(0, 9, 9),
        ]);

        unsafe { check_keepend() };

        let state = unsafe { &*CURRENT_STATE.get_mut() };
        assert_eq!(state[1].si_m_endpos, LposT { lnum: 9, col: 9 }, "untouched");
    }

    /// A `keepend` item pulls every later item's end positions back to
    /// its own, and marks them as ending there.
    #[test]
    fn check_keepend_propagates_the_end_to_contained_items() {
        let _lock = crate::globals::global_state_test_lock();
        let _k = KeependLevelGuard::install(0);
        let mut inner = ends_at(0, 9, 9);
        inner.si_ends = false;
        let _g = CurrentStackGuard::install(vec![ends_at(hl_flags::KEEPEND, 1, 5), inner]);

        unsafe { check_keepend() };

        let state = unsafe { &*CURRENT_STATE.get_mut() };
        assert_eq!(
            state[1].si_m_endpos,
            LposT { lnum: 1, col: 5 },
            "contained item is limited to the keepend item's end"
        );
        assert_eq!(state[1].si_eoe_pos, LposT { lnum: 1, col: 5 });
        assert!(state[1].si_ends, "and is now marked as ending there");
        assert_eq!(
            state[0].si_m_endpos,
            LposT { lnum: 1, col: 5 },
            "the keepend item itself is unchanged"
        );
    }

    /// `extend` explicitly ignores a `keepend`, so a `keepend` item
    /// *below* the last `extend` item has no effect at all.
    #[test]
    fn check_keepend_ignores_a_keepend_below_the_last_extend() {
        let _lock = crate::globals::global_state_test_lock();
        let _k = KeependLevelGuard::install(0);
        let _g = CurrentStackGuard::install(vec![
            ends_at(hl_flags::KEEPEND, 1, 5),
            ends_at(hl_flags::EXTEND, 4, 2),
            ends_at(0, 9, 9),
        ]);

        unsafe { check_keepend() };

        let state = unsafe { &*CURRENT_STATE.get_mut() };
        assert_eq!(
            state[2].si_m_endpos,
            LposT { lnum: 9, col: 9 },
            "the scan restarts at the extend item, so the keepend below it is skipped"
        );
    }

    /// With several `keepend` items the earliest end wins, since each
    /// one only narrows the limit.
    #[test]
    fn check_keepend_keeps_the_earliest_end_position() {
        let _lock = crate::globals::global_state_test_lock();
        let _k = KeependLevelGuard::install(0);
        let _g = CurrentStackGuard::install(vec![
            ends_at(hl_flags::KEEPEND, 3, 0),
            ends_at(hl_flags::KEEPEND, 2, 7),
            ends_at(0, 9, 9),
        ]);

        unsafe { check_keepend() };

        let state = unsafe { &*CURRENT_STATE.get_mut() };
        assert_eq!(
            state[2].si_m_endpos,
            LposT { lnum: 2, col: 7 },
            "line 2 beats line 3"
        );
    }

    // ---- push_current_state ----

    /// A pushed state carries the given index and is otherwise
    /// cleared, matching the original's own `CLEAR_POINTER` before it
    /// assigns `si_idx`.
    #[test]
    fn push_current_state_appends_a_cleared_item_with_the_given_index() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = CurrentStackGuard::install(Vec::new());

        unsafe { push_current_state(7) };

        assert_eq!(unsafe { current_state_len() }, 1);
        let state = unsafe { &*CURRENT_STATE.get_mut() };
        assert_eq!(state[0].si_idx, 7);
        assert_eq!(state[0].si_flags, 0, "the rest of the item is cleared");
        assert_eq!(state[0].si_id, 0);
        assert!(!state[0].si_ends);
        assert!(state[0].si_cont_list.is_empty());
        assert!(state[0].si_extmatch.is_null());
    }

    /// States stack rather than replace, innermost last, and the
    /// keyword sentinel is stored as-is.
    #[test]
    fn push_current_state_stacks_items_in_order() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = CurrentStackGuard::install(Vec::new());

        unsafe {
            push_current_state(2);
            push_current_state(KEYWORD_IDX);
            push_current_state(5);
        }

        let state = unsafe { &*CURRENT_STATE.get_mut() };
        let idxs: Vec<i32> = state.iter().map(|si| si.si_idx).collect();
        assert_eq!(idxs, vec![2, KEYWORD_IDX, 5]);
    }

    /// A pushed state does not fold unless its flags say so, so
    /// pushing alone must not change the fold level.
    #[test]
    fn push_current_state_does_not_change_the_fold_level() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = CurrentStackGuard::install(Vec::new());

        unsafe {
            push_current_state(1);
            push_current_state(2);
        }
        assert_eq!(unsafe { syn_cur_foldlevel() }, 0);
    }

    // ---- syn_cur_foldlevel ----

    #[test]
    fn syn_cur_foldlevel_is_zero_without_any_state() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = CurrentStackGuard::install(Vec::new());

        assert_eq!(unsafe { syn_cur_foldlevel() }, 0);
    }

    /// Only items carrying `HL_FOLD` count, so a deep stack of
    /// non-folding items is still level zero.
    #[test]
    fn syn_cur_foldlevel_ignores_items_without_the_fold_flag() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = CurrentStackGuard::install(stack_with_flags(&[
            hl_flags::CONTAINED,
            hl_flags::TRANSP,
            hl_flags::CONCEAL,
        ]));

        assert_eq!(unsafe { syn_cur_foldlevel() }, 0, "no item folds");
    }

    /// Every folding item on the stack adds one level, so the result
    /// is the number of folding items - not the stack depth, and not
    /// merely whether one was found.
    #[test]
    fn syn_cur_foldlevel_counts_each_folding_item() {
        let _lock = crate::globals::global_state_test_lock();
        let _g = CurrentStackGuard::install(stack_with_flags(&[
            hl_flags::FOLD,
            hl_flags::CONTAINED,
            hl_flags::FOLD | hl_flags::TRANSP,
            hl_flags::CONCEAL,
            hl_flags::FOLD,
        ]));

        assert_eq!(
            unsafe { syn_cur_foldlevel() },
            3,
            "3 of the 5 items fold, and FOLD combines with other flags"
        );
    }

    // ---- idl cache ----

    #[test]
    fn idl_cache_clear_is_a_noop_without_a_cache() {
        let mut block = crate::buffer_defs::SynblockT::default();
        assert!(block.b_idlist_cache.is_null());
        unsafe { idl_cache_clear(&mut block) };
        assert!(block.b_idlist_cache.is_null());
    }

    /// Clearing releases the cache and nulls the field, so a second
    /// call is safe rather than freeing twice.
    #[test]
    fn idl_cache_clear_releases_the_cache_and_is_idempotent() {
        let mut block = crate::buffer_defs::SynblockT::default();

        let mut cache = IdlCacheT::default();
        // Give two entries owned payloads, so a missed free shows up
        // under Miri's leak check rather than passing silently.
        cache.ent[0].idl_ids = vec![1, 2, 3];
        cache.ent[3].idl_ids = vec![4, 5];
        cache.last = 3;
        block.b_idlist_cache = Box::into_raw(Box::new(cache));

        unsafe { idl_cache_clear(&mut block) };
        assert!(block.b_idlist_cache.is_null());

        // Safe to repeat.
        unsafe { idl_cache_clear(&mut block) };
        assert!(block.b_idlist_cache.is_null());
    }

    /// A default cache starts empty: no keys, no IDs, and not usable
    /// until something fills it in.
    #[test]
    fn idl_cache_default_starts_empty() {
        let cache = IdlCacheT::default();
        assert_eq!(cache.ent.len(), IDL_CACHE_SIZE);
        assert_eq!((cache.round, cache.last), (0, 0));
        for e in &cache.ent {
            assert!(e.idl_key.is_null());
            assert!(e.idl_ids.is_empty());
            assert_eq!(e.idl_marker, 0);
            assert!(!e.idl_usable);
        }
    }

    #[test]
    fn idl_cache_size_matches_the_original() {
        assert_eq!(IDL_CACHE_SIZE, 16);
    }

    // ---- init_syn_patterns / idl_id_cmp / reset_expand_highlight ----

    /// Sets the grow size WITHOUT emptying the table - a translation
    /// via `ga_init` would have cleared it, since the original calls
    /// `ga_set_growsize` rather than `ga_init`.
    #[test]
    fn init_syn_patterns_sets_growsize_without_clearing() {
        let _lock = crate::globals::global_state_test_lock();
        let fx = ClusterFixture::new(&[]);

        fx.block_mut().b_syn_patterns.items.push(SynpatT::default());
        fx.block_mut().b_syn_patterns.ga_growsize = 1;

        unsafe { init_syn_patterns() };

        assert_eq!(fx.block().b_syn_patterns.ga_growsize, 10);
        assert_eq!(
            fx.block().b_syn_patterns.ga_len(),
            1,
            "the existing pattern must survive"
        );
    }

    /// A binary search has to find an id wherever it sits, so this
    /// checks every position of an odd- and an even-length set rather
    /// than just the middle one.
    #[test]
    fn idl_contains_finds_an_id_at_any_position() {
        for ids in [vec![3i16, 7, 11, 20, 42], vec![3i16, 7, 11, 20]] {
            let e = IdlEntryT {
                idl_ids: ids.clone(),
                ..Default::default()
            };
            for &id in &ids {
                assert!(idl_contains(&e, id), "{id} missing from {ids:?}");
            }
        }
    }

    /// Values below, above and between the stored ids must all miss -
    /// an off-by-one in the search bounds would report one of these
    /// as present.
    #[test]
    fn idl_contains_rejects_ids_outside_the_set() {
        let e = IdlEntryT {
            idl_ids: vec![3, 7, 11, 20, 42],
            ..Default::default()
        };
        for id in [i16::MIN, 2, 4, 8, 10, 12, 19, 21, 41, 43, i16::MAX] {
            assert!(!idl_contains(&e, id), "{id} should not be found");
        }
    }

    #[test]
    fn idl_contains_is_false_for_an_empty_set() {
        let e = IdlEntryT::default();
        assert!(!idl_contains(&e, 0));
        assert!(!idl_contains(&e, 7));
    }

    /// The sort order the search relies on is the one `idl_id_cmp`
    /// defines, so a set built through it stays searchable - including
    /// the negative ids `idl_id_cmp` is careful about.
    #[test]
    fn idl_contains_agrees_with_idl_id_cmp_ordering() {
        let mut ids = vec![42i16, -30, 7, -1, 11, 3];
        ids.sort_by(|a, b| idl_id_cmp(*a, *b).cmp(&0));
        let e = IdlEntryT {
            idl_ids: ids.clone(),
            ..Default::default()
        };
        for &id in &ids {
            assert!(idl_contains(&e, id), "{id} missing after sorting");
        }
        assert!(!idl_contains(&e, -2));
    }

    #[test]
    fn idl_id_cmp_orders_ids_ascending() {
        assert!(idl_id_cmp(1, 2) < 0);
        assert!(idl_id_cmp(2, 1) > 0);
        assert_eq!(idl_id_cmp(5, 5), 0);

        let mut v = [30i16, -1, 10, 20];
        v.sort_by(|a, b| idl_id_cmp(*a, *b).cmp(&0));
        assert_eq!(v, [-1, 10, 20, 30]);
    }

    /// The subtraction happens in i32, so two far-apart i16 values
    /// give the right sign instead of wrapping.
    #[test]
    fn idl_id_cmp_does_not_overflow_on_extreme_ids() {
        assert!(idl_id_cmp(i16::MIN, i16::MAX) < 0);
        assert!(idl_id_cmp(i16::MAX, i16::MIN) > 0);
    }

    #[test]
    fn reset_expand_highlight_clears_all_three_counters() {
        let _lock = crate::globals::global_state_test_lock();
        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        let saved = (g.include_none, g.include_default, g.include_link);

        g.include_none = 1;
        g.include_default = 1;
        g.include_link = 1;

        unsafe { reset_expand_highlight() };

        let g = unsafe { crate::globals::GLOBALS.get_mut() };
        assert_eq!((g.include_none, g.include_default, g.include_link), (0, 0, 0));

        (g.include_none, g.include_default, g.include_link) = saved;
    }

    // ---- syn_add_start_off / syn_add_end_off ----

    /// A match spanning the given start/end positions on line 1.
    fn offset_match(
        start_col: crate::pos_defs::ColnrT,
        end_col: crate::pos_defs::ColnrT,
    ) -> crate::regexp_defs::RegmmatchT {
        let mut rm = crate::regexp_defs::RegmmatchT::default();
        rm.startpos[0] = LposT { lnum: 1, col: start_col };
        rm.endpos[0] = LposT { lnum: 1, col: end_col };
        rm
    }

    /// A pattern with one offset slot set, and optionally its
    /// "measure from the other end" flag bit.
    fn offset_pattern(idx: usize, off: i32, flag_bit: Option<usize>) -> SynpatT {
        let mut spp = SynpatT::default();
        spp.sp_offsets[idx] = off;
        if let Some(bit) = flag_bit {
            spp.sp_off_flags = 1 << bit;
        }
        spp
    }

    /// Without its flag bit, an end offset is measured from the
    /// match's END; the offset moves by CHARACTERS.
    #[test]
    fn syn_add_end_off_measures_from_the_match_end_by_default() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = syntax_test_buf(b"abcdefgh\0");
        let _g = SynBufGuard::install(std::ptr::from_mut(&mut buf), 1);

        let rm = offset_match(2, 5);
        let spp = offset_pattern(spo::ME_OFF, 2, None);
        let mut result = LposT::default();
        unsafe { syn_add_end_off(&mut result, &rm, &spp, spo::ME_OFF, 0) };

        assert_eq!((result.lnum, result.col), (1, 7), "end 5 plus 2 characters");
        close_syntax_test_buf(buf);
    }

    /// With its LOW flag bit set, the offset is measured from the
    /// match's START instead, and `extra` is added.
    #[test]
    fn syn_add_end_off_measures_from_the_start_when_flagged() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = syntax_test_buf(b"abcdefgh\0");
        let _g = SynBufGuard::install(std::ptr::from_mut(&mut buf), 1);

        let rm = offset_match(2, 5);
        let spp = offset_pattern(spo::ME_OFF, 1, Some(spo::ME_OFF));
        let mut result = LposT::default();
        unsafe { syn_add_end_off(&mut result, &rm, &spp, spo::ME_OFF, 3) };

        assert_eq!(result.col, 6, "start 2 plus offset 1 plus extra 3");
        close_syntax_test_buf(buf);
    }

    /// A negative offset walks backwards, and stops at the start of
    /// the line rather than running before it.
    #[test]
    fn syn_add_end_off_walks_backwards_and_stops_at_the_line_start() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = syntax_test_buf(b"abcdefgh\0");
        let _g = SynBufGuard::install(std::ptr::from_mut(&mut buf), 1);

        let rm = offset_match(0, 3);
        let mut result = LposT::default();

        let spp = offset_pattern(spo::ME_OFF, -2, None);
        unsafe { syn_add_end_off(&mut result, &rm, &spp, spo::ME_OFF, 0) };
        assert_eq!(result.col, 1, "3 back 2");

        let spp = offset_pattern(spo::ME_OFF, -99, None);
        unsafe { syn_add_end_off(&mut result, &rm, &spp, spo::ME_OFF, 0) };
        assert_eq!(result.col, 0, "clamped at the line start");
        close_syntax_test_buf(buf);
    }

    /// Walking forwards stops at the end of the line.
    #[test]
    fn syn_add_end_off_stops_at_the_line_end() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = syntax_test_buf(b"abc\0");
        let _g = SynBufGuard::install(std::ptr::from_mut(&mut buf), 1);

        let rm = offset_match(0, 1);
        let spp = offset_pattern(spo::ME_OFF, 99, None);
        let mut result = LposT::default();
        unsafe { syn_add_end_off(&mut result, &rm, &spp, spo::ME_OFF, 0) };

        assert_eq!(result.col, 3, "stopped at the NUL");
        close_syntax_test_buf(buf);
    }

    /// A line past the end of the buffer zeroes the column and SKIPS
    /// the offset walk - the original's second test is an `else if` -
    /// and the line number is NOT clamped.
    #[test]
    fn syn_add_end_off_zeroes_the_column_past_the_last_line() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = syntax_test_buf(b"abc\0");
        let _g = SynBufGuard::install(std::ptr::from_mut(&mut buf), 1);

        let mut rm = crate::regexp_defs::RegmmatchT::default();
        rm.endpos[0] = LposT { lnum: 99, col: 2 };
        let spp = offset_pattern(spo::ME_OFF, 2, None);
        let mut result = LposT::default();
        unsafe { syn_add_end_off(&mut result, &rm, &spp, spo::ME_OFF, 0) };

        assert_eq!(result.col, 0, "column zeroed, offset not applied");
        assert_eq!(result.lnum, 99, "line NOT clamped, unlike syn_add_start_off");
        close_syntax_test_buf(buf);
    }

    /// A start offset uses the HIGH flag bit - shifted by SPO_COUNT -
    /// so each offset has its own "from which end" bit.
    #[test]
    fn syn_add_start_off_uses_the_high_flag_bit() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = syntax_test_buf(b"abcdefgh\0");
        let _g = SynBufGuard::install(std::ptr::from_mut(&mut buf), 1);

        let rm = offset_match(2, 5);
        let mut result = LposT::default();

        // Low bit alone must NOT switch it to measuring from the end.
        let spp = offset_pattern(spo::MS_OFF, 1, Some(spo::MS_OFF));
        unsafe { syn_add_start_off(&mut result, &rm, &spp, spo::MS_OFF, 3) };
        assert_eq!(result.col, 3, "still from the start: 2 plus 1");

        // The high bit does.
        let spp = offset_pattern(spo::MS_OFF, 1, Some(spo::MS_OFF + spo::COUNT));
        unsafe { syn_add_start_off(&mut result, &rm, &spp, spo::MS_OFF, 3) };
        assert_eq!(result.col, 8, "from the end: 5 plus 1 plus extra 3, clamped");
        close_syntax_test_buf(buf);
    }

    /// A line past the end is CLAMPED to the last line and the column
    /// set to its length - the opposite of syn_add_end_off.
    #[test]
    fn syn_add_start_off_clamps_past_the_last_line() {
        let _lock = crate::globals::global_state_test_lock();
        let mut buf = syntax_test_buf(b"abc\0");
        let _g = SynBufGuard::install(std::ptr::from_mut(&mut buf), 1);

        let mut rm = crate::regexp_defs::RegmmatchT::default();
        rm.startpos[0] = LposT { lnum: 99, col: 0 };
        let spp = offset_pattern(spo::MS_OFF, 0, None);
        let mut result = LposT::default();
        unsafe { syn_add_start_off(&mut result, &rm, &spp, spo::MS_OFF, 0) };

        assert_eq!(result.lnum, 1, "clamped to the last line");
        assert_eq!(result.col, 3, "column set to that line's length");
        close_syntax_test_buf(buf);
    }

    // ---- syntax_present / syntime_clear ----

    #[test]
    fn syntax_present_is_false_for_an_empty_syntax_block() {
        let block = crate::buffer_defs::SynblockT::default();
        assert!(!syntax_present(&block));
    }

    /// Any ONE of the four sources of syntax items is enough, so a
    /// check that missed one would fail here.
    #[test]
    fn syntax_present_is_true_for_each_source_on_its_own() {
        let mut by_pattern = crate::buffer_defs::SynblockT::default();
        by_pattern.b_syn_patterns.items.push(SynpatT::default());
        assert!(syntax_present(&by_pattern), "patterns alone");

        let mut by_cluster = crate::buffer_defs::SynblockT::default();
        by_cluster.b_syn_clusters.items.push(SynClusterT::default());
        assert!(syntax_present(&by_cluster), "clusters alone");

        let mut by_keyword = crate::buffer_defs::SynblockT::default();
        by_keyword.b_keywtab.ht_used = 1;
        assert!(syntax_present(&by_keyword), "keyword table alone");

        let mut by_keyword_ic = crate::buffer_defs::SynblockT::default();
        by_keyword_ic.b_keywtab_ic.ht_used = 1;
        assert!(syntax_present(&by_keyword_ic), "ignore-case keyword table alone");
    }

    /// A pattern with recorded timings has them all reset.
    #[test]
    fn syntime_clear_resets_every_pattern() {
        let _lock = crate::globals::global_state_test_lock();
        let fx = ClusterFixture::new(&[]);

        let block = fx.block_mut();
        for total in [111u64, 222] {
            block.b_syn_patterns.items.push(SynpatT {
                sp_time: crate::buffer_defs::SynTimeT {
                    total,
                    slowest: total,
                    count: 5,
                    match_: 3,
                },
                ..Default::default()
            });
        }

        unsafe { syntime_clear() };

        for spp in &fx.block().b_syn_patterns.items {
            assert_eq!(spp.sp_time.total, crate::profile::profile_zero());
            assert_eq!(spp.sp_time.slowest, crate::profile::profile_zero());
            assert_eq!(spp.sp_time.count, 0);
            assert_eq!(spp.sp_time.match_, 0);
        }
    }

    /// With no syntax at all the function returns early. Nothing
    /// observable changes either way here, but the early return is
    /// what keeps the original from printing its "no items" message,
    /// so the guard is exercised deliberately.
    #[test]
    fn syntime_clear_returns_early_without_syntax() {
        let _lock = crate::globals::global_state_test_lock();
        let fx = ClusterFixture::new(&[]);
        assert!(!syntax_present(fx.block()));
        unsafe { syntime_clear() };
        assert!(fx.block().b_syn_patterns.items.is_empty());
    }

    #[test]
    fn spo_and_sptype_values_match_the_original() {
        assert_eq!((spo::MS_OFF, spo::ME_OFF, spo::HS_OFF, spo::HE_OFF), (0, 1, 2, 3));
        assert_eq!((spo::RS_OFF, spo::RE_OFF, spo::LC_OFF, spo::COUNT), (4, 5, 6, 7));
        assert_eq!(
            (sptype::MATCH, sptype::START, sptype::END, sptype::SKIP),
            (1, 2, 3, 4)
        );
    }

    // ---- limit_pos_zero / syn_clear_time ----

    /// An unset position (line 0) adopts the limit wholesale. Plain
    /// `limit_pos` would leave it alone, since line 0 always compares
    /// as earlier.
    #[test]
    fn limit_pos_zero_adopts_the_limit_for_an_unset_position() {
        let limit = LposT { lnum: 7, col: 3 };
        let mut pos = LposT { lnum: 0, col: 99 };
        limit_pos_zero(&mut pos, &limit);
        assert_eq!((pos.lnum, pos.col), (7, 3));

        // Contrast: limit_pos leaves the same position untouched.
        let mut plain = LposT { lnum: 0, col: 99 };
        limit_pos(&mut plain, &limit);
        assert_eq!((plain.lnum, plain.col), (0, 99));
    }

    /// With a set line number it behaves exactly like `limit_pos`.
    #[test]
    fn limit_pos_zero_defers_to_limit_pos_for_a_set_position() {
        let limit = LposT { lnum: 7, col: 3 };

        let mut later = LposT { lnum: 9, col: 1 };
        limit_pos_zero(&mut later, &limit);
        assert_eq!((later.lnum, later.col), (7, 3), "later line pulled back");

        let mut same = LposT { lnum: 7, col: 8 };
        limit_pos_zero(&mut same, &limit);
        assert_eq!((same.lnum, same.col), (7, 3), "same line, column clamped");

        let mut earlier = LposT { lnum: 2, col: 99 };
        limit_pos_zero(&mut earlier, &limit);
        assert_eq!((earlier.lnum, earlier.col), (2, 99), "earlier line untouched");
    }

    #[test]
    fn syn_clear_time_resets_every_counter() {
        let mut st = crate::buffer_defs::SynTimeT {
            total: 1234,
            slowest: 567,
            count: 89,
            match_: 10,
        };
        syn_clear_time(&mut st);
        assert_eq!(st.total, crate::profile::profile_zero());
        assert_eq!(st.slowest, crate::profile::profile_zero());
        assert_eq!(st.count, 0);
        assert_eq!(st.match_, 0);
    }

    // ---- syn_incl_toplevel ----

    #[test]
    fn syn_incl_toplevel_does_nothing_for_an_already_contained_item() {
        let _lock = crate::globals::global_state_test_lock();
        let fx = ClusterFixture::new(&[b"top"]);
        fx.block_mut().b_syn_topgrp = SYNID_CLUSTER;

        let mut flags = hl_flags::CONTAINED | hl_flags::FOLD;
        unsafe { syn_incl_toplevel(42, &mut flags) };

        assert_eq!(flags, hl_flags::CONTAINED | hl_flags::FOLD, "flags untouched");
        assert!(
            fx.block().b_syn_clusters.get(0).unwrap().scl_list.is_empty(),
            "no group added"
        );
    }

    #[test]
    fn syn_incl_toplevel_does_nothing_without_a_top_group() {
        let _lock = crate::globals::global_state_test_lock();
        let fx = ClusterFixture::new(&[b"top"]);
        fx.block_mut().b_syn_topgrp = 0;

        let mut flags = 0;
        unsafe { syn_incl_toplevel(42, &mut flags) };

        assert_eq!(flags, 0);
        assert!(fx.block().b_syn_clusters.get(0).unwrap().scl_list.is_empty());
    }

    /// A top group below `SYNID_CLUSTER` is a plain highlight group,
    /// not a cluster, so the flags are set but no list is touched.
    #[test]
    fn syn_incl_toplevel_sets_flags_only_for_a_non_cluster_top_group() {
        let _lock = crate::globals::global_state_test_lock();
        let fx = ClusterFixture::new(&[b"top"]);
        fx.block_mut().b_syn_topgrp = SYNID_TOP;

        let mut flags = 0;
        unsafe { syn_incl_toplevel(42, &mut flags) };

        assert_eq!(flags, hl_flags::CONTAINED | hl_flags::INCLUDED_TOPLEVEL);
        assert!(fx.block().b_syn_clusters.get(0).unwrap().scl_list.is_empty());
    }

    /// The real path: the item is added to the top-level cluster's own
    /// group list, and the flags are set.
    #[test]
    fn syn_incl_toplevel_adds_the_item_to_the_top_cluster() {
        let _lock = crate::globals::global_state_test_lock();
        let fx = ClusterFixture::new(&[b"zero", b"top"]);
        // The second cluster is the top group.
        fx.block_mut().b_syn_topgrp = SYNID_CLUSTER + 1;

        let mut flags = hl_flags::FOLD;
        unsafe { syn_incl_toplevel(42, &mut flags) };

        assert_eq!(
            flags,
            hl_flags::FOLD | hl_flags::CONTAINED | hl_flags::INCLUDED_TOPLEVEL
        );
        let block = fx.block();
        assert_eq!(block.b_syn_clusters.get(1).unwrap().scl_list, vec![42]);
        // The other cluster is left alone.
        assert!(block.b_syn_clusters.get(0).unwrap().scl_list.is_empty());
    }

    /// Adding merges into whatever the cluster already held, keeping
    /// it sorted and free of duplicates.
    #[test]
    fn syn_incl_toplevel_merges_into_an_existing_group_list() {
        let _lock = crate::globals::global_state_test_lock();
        let fx = ClusterFixture::new(&[b"top"]);
        fx.block_mut().b_syn_topgrp = SYNID_CLUSTER;
        fx.block_mut().b_syn_clusters.items[0].scl_list = vec![10, 50];

        let mut flags = 0;
        unsafe { syn_incl_toplevel(42, &mut flags) };
        assert_eq!(fx.block().b_syn_clusters.get(0).unwrap().scl_list, vec![10, 42, 50]);

        // Adding the same id again does not duplicate it.
        let mut flags2 = 0;
        unsafe { syn_incl_toplevel(42, &mut flags2) };
        assert_eq!(fx.block().b_syn_clusters.get(0).unwrap().scl_list, vec![10, 42, 50]);
    }

    #[test]
    fn hl_flag_values_match_the_original() {
        // Spot-check the ends and a few in between; each is a
        // distinct single bit.
        assert_eq!(hl_flags::CONTAINED, 0x01);
        assert_eq!(hl_flags::TRANSP, 0x02);
        assert_eq!(hl_flags::MATCH, 0x40);
        assert_eq!(hl_flags::KEEPEND, 0x400);
        assert_eq!(hl_flags::FOLD, 0x2000);
        assert_eq!(hl_flags::TRANS_CONT, 0x1_0000);
        assert_eq!(hl_flags::CONCEALENDS, 0x4_0000);
        assert_eq!(hl_flags::INCLUDED_TOPLEVEL, 0x8_0000);
    }

    // ---- syn_combine_list ----

    #[test]
    fn syn_combine_list_leaves_the_first_list_alone_for_an_empty_second() {
        let mut a = vec![3i16, 1];
        syn_combine_list(&mut a, Vec::new(), ClusterOp::Add);
        assert_eq!(a, vec![3, 1], "an empty second list is a no-op, unsorted");
    }

    #[test]
    fn syn_combine_list_replace_overwrites_regardless_of_the_first_list() {
        let mut a = vec![1i16, 2, 3];
        syn_combine_list(&mut a, vec![9, 8], ClusterOp::Replace);
        assert_eq!(a, vec![9, 8], "replace takes the second list verbatim");
    }

    #[test]
    fn syn_combine_list_add_to_an_empty_first_list_takes_the_second() {
        let mut a: Vec<i16> = Vec::new();
        syn_combine_list(&mut a, vec![4, 2], ClusterOp::Add);
        assert_eq!(a, vec![4, 2]);
    }

    /// Subtracting from an empty list leaves it empty rather than
    /// adopting the second list.
    #[test]
    fn syn_combine_list_subtract_from_an_empty_first_list_stays_empty() {
        let mut a: Vec<i16> = Vec::new();
        syn_combine_list(&mut a, vec![4, 2], ClusterOp::Subtract);
        assert!(a.is_empty());
    }

    /// Adding merges both lists in sorted order, and an id present in
    /// both appears once, not twice.
    #[test]
    fn syn_combine_list_add_merges_sorted_and_deduplicates_the_overlap() {
        let mut a = vec![5i16, 1, 3];
        syn_combine_list(&mut a, vec![4, 3, 2], ClusterOp::Add);
        assert_eq!(a, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn syn_combine_list_subtract_removes_only_the_shared_ids() {
        let mut a = vec![5i16, 1, 3, 2];
        syn_combine_list(&mut a, vec![3, 5], ClusterOp::Subtract);
        assert_eq!(a, vec![1, 2]);
    }

    /// Ids in the second list that aren't in the first are simply
    /// skipped by a subtract, not added.
    #[test]
    fn syn_combine_list_subtract_ignores_ids_absent_from_the_first() {
        let mut a = vec![1i16, 2];
        syn_combine_list(&mut a, vec![7, 9], ClusterOp::Subtract);
        assert_eq!(a, vec![1, 2]);
    }

    /// Subtracting everything yields an empty list.
    #[test]
    fn syn_combine_list_subtract_can_empty_the_first_list() {
        let mut a = vec![2i16, 1];
        syn_combine_list(&mut a, vec![1, 2], ClusterOp::Subtract);
        assert!(a.is_empty());
    }

    /// A duplicate within the first list is only cancelled once per
    /// matching id in the second, matching the original's own
    /// one-step-each merge.
    #[test]
    fn syn_combine_list_subtract_cancels_one_duplicate_per_match() {
        let mut a = vec![5i16, 5, 1];
        syn_combine_list(&mut a, vec![5], ClusterOp::Subtract);
        assert_eq!(a, vec![1, 5], "only one of the two 5s is removed");
    }

    /// Leftovers from the first list are always kept; leftovers from
    /// the second are kept only when adding.
    #[test]
    fn syn_combine_list_keeps_first_list_leftovers_but_not_second_on_subtract() {
        let mut add = vec![1i16];
        syn_combine_list(&mut add, vec![2, 3], ClusterOp::Add);
        assert_eq!(add, vec![1, 2, 3]);

        let mut sub = vec![1i16, 8, 9];
        syn_combine_list(&mut sub, vec![2], ClusterOp::Subtract);
        assert_eq!(sub, vec![1, 8, 9]);
    }

    // ---- syn_check_cluster / syn_add_cluster ----

    /// Reads back the cluster table's names, for asserting that an
    /// add really did (or didn't) happen. Names are stored
    /// NUL-terminated, so the expected values carry a NUL too.
    fn cluster_names(block: &crate::buffer_defs::SynblockT) -> Vec<Vec<u8>> {
        block
            .b_syn_clusters
            .items
            .iter()
            .map(|c| c.scl_name.clone().unwrap_or_default())
            .collect()
    }

    #[test]
    fn syn_check_cluster_returns_an_existing_cluster_without_adding() {
        let _lock = crate::globals::global_state_test_lock();
        let fx = ClusterFixture::new(&[b"alpha", b"beta"]);

        let id = unsafe { syn_check_cluster(b"beta", 4) };
        assert_eq!(id, SYNID_CLUSTER + 1);
        assert_eq!(
            cluster_names(fx.block()),
            vec![b"alpha\0".to_vec(), b"beta\0".to_vec()],
            "an existing cluster must not be appended a second time"
        );
    }

    /// The lookup is case-insensitive, so a differently-cased name
    /// finds the existing cluster rather than creating a new one.
    #[test]
    fn syn_check_cluster_matches_an_existing_cluster_case_insensitively() {
        let _lock = crate::globals::global_state_test_lock();
        let fx = ClusterFixture::new(&[b"alpha"]);

        assert_eq!(unsafe { syn_check_cluster(b"ALPHA", 5) }, SYNID_CLUSTER);
        assert_eq!(fx.block().b_syn_clusters.ga_len(), 1);
    }

    #[test]
    fn syn_check_cluster_appends_a_new_cluster_when_there_is_none() {
        let _lock = crate::globals::global_state_test_lock();
        let fx = ClusterFixture::new(&[b"alpha"]);

        let id = unsafe { syn_check_cluster(b"gamma", 5) };
        assert_eq!(id, SYNID_CLUSTER + 1);

        let block = fx.block();
        assert_eq!(cluster_names(block), vec![b"alpha\0".to_vec(), b"gamma\0".to_vec()]);
        // The new entry carries both its name and the uppercase form
        // the lookup matches against, each NUL-terminated.
        let added = block.b_syn_clusters.get(1).unwrap();
        assert_eq!(added.scl_name_u.as_deref(), Some(b"GAMMA\0".as_slice()));
        assert!(added.scl_list.is_empty());
        // ...and is immediately findable.
        assert_eq!(unsafe { syn_scl_name2id(b"gamma") }, SYNID_CLUSTER + 1);
    }

    /// `len` bounds the name, so trailing text in the buffer is not
    /// part of the cluster that gets created.
    #[test]
    fn syn_check_cluster_uses_only_the_first_len_bytes() {
        let _lock = crate::globals::global_state_test_lock();
        let fx = ClusterFixture::new(&[]);

        assert_eq!(unsafe { syn_check_cluster(b"abc,rest", 3) }, SYNID_CLUSTER);
        assert_eq!(cluster_names(fx.block()), vec![b"abc\0".to_vec()]);
    }

    /// Adding the "Spell"/"NoSpell" clusters records their ids on the
    /// syntax block, case-insensitively, and the two are kept apart.
    #[test]
    fn syn_add_cluster_records_the_spell_cluster_ids() {
        let _lock = crate::globals::global_state_test_lock();
        let fx = ClusterFixture::new(&[b"filler"]);

        assert_eq!(fx.block().b_spell_cluster_id, 0);
        assert_eq!(fx.block().b_nospell_cluster_id, 0);

        let spell = unsafe { syn_add_cluster(b"sPeLl".to_vec()) };
        let nospell = unsafe { syn_add_cluster(b"NOSPELL".to_vec()) };

        assert_eq!(spell, SYNID_CLUSTER + 1);
        assert_eq!(nospell, SYNID_CLUSTER + 2);
        let block = fx.block();
        assert_eq!(block.b_spell_cluster_id, SYNID_CLUSTER + 1);
        // "NoSpell" must not have overwritten the "Spell" id.
        assert_eq!(block.b_nospell_cluster_id, SYNID_CLUSTER + 2);
    }

    /// An ordinary cluster name leaves both spell ids alone.
    #[test]
    fn syn_add_cluster_leaves_the_spell_ids_alone_for_other_names() {
        let _lock = crate::globals::global_state_test_lock();
        let fx = ClusterFixture::new(&[]);

        assert_eq!(unsafe { syn_add_cluster(b"Spelling".to_vec()) }, SYNID_CLUSTER);
        let block = fx.block();
        assert_eq!(block.b_spell_cluster_id, 0);
        assert_eq!(block.b_nospell_cluster_id, 0);
    }

    /// A full table refuses the add (returning 0) and is left
    /// untouched, rather than growing past the id space.
    #[test]
    fn syn_add_cluster_refuses_to_grow_past_max_cluster_id() {
        let _lock = crate::globals::global_state_test_lock();
        let fx = ClusterFixture::new(&[]);

        // Fill the table right up to the limit.
        {
            let block = fx.block_mut();
            block
                .b_syn_clusters
                .items
                .resize_with(MAX_CLUSTER_ID as usize, SynClusterT::default);
        }

        assert_eq!(unsafe { syn_add_cluster(b"toomany".to_vec()) }, 0);
        assert_eq!(
            fx.block().b_syn_clusters.ga_len(),
            MAX_CLUSTER_ID,
            "a refused add must not append anything"
        );
    }

    // ---- syn_scl_name2id / syn_scl_namen2id ----

    /// A window whose syntax block holds the given clusters,
    /// installed as `curwin` for the fixture's lifetime.
    ///
    /// Both the window and the block are handed to raw pointers via
    /// `Box::into_raw` rather than kept as live `Box` bindings. That
    /// matters: the code under test writes through
    /// `curwin->w_s`, and under Tree Borrows such a write through a
    /// derived pointer would disable an owning `Box`'s own tag,
    /// making that `Box`'s eventual drop undefined behaviour. Owning
    /// the allocations as raw pointers and reclaiming them here
    /// keeps every access in one lineage.
    struct ClusterFixture {
        win: *mut crate::buffer_defs::WinT,
        block: *mut crate::buffer_defs::SynblockT,
        prev_curwin: *mut crate::buffer_defs::WinT,
    }

    impl ClusterFixture {
        /// Builds the clusters exactly as the real creation path
        /// stores them: both name fields NUL-terminated.
        fn new(names: &[&[u8]]) -> Self {
            let mut block = Box::new(crate::buffer_defs::SynblockT::default());
            block.b_syn_clusters.items = names
                .iter()
                .map(|n| SynClusterT {
                    scl_name: Some(crate::strings::xstrnsave(n, n.len())),
                    scl_name_u: Some(crate::strings::vim_strsave_up(n)),
                    scl_list: Vec::new(),
                })
                .collect();
            let block = Box::into_raw(block);

            let mut win = Box::new(crate::buffer_defs::WinT::default());
            win.w_s = block;
            let win = Box::into_raw(win);

            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            let prev_curwin = g.curwin;
            g.curwin = win;
            Self { win, block, prev_curwin }
        }

        fn block(&self) -> &crate::buffer_defs::SynblockT {
            unsafe { &*self.block }
        }

        #[allow(clippy::mut_from_ref)]
        fn block_mut(&self) -> &mut crate::buffer_defs::SynblockT {
            unsafe { &mut *self.block }
        }
    }

    impl Drop for ClusterFixture {
        fn drop(&mut self) {
            unsafe { crate::globals::GLOBALS.get_mut() }.curwin = self.prev_curwin;
            unsafe {
                drop(Box::from_raw(self.win));
                drop(Box::from_raw(self.block));
            }
        }
    }

    #[test]
    fn syn_scl_name2id_returns_zero_when_there_are_no_clusters() {
        let _lock = crate::globals::global_state_test_lock();
        let _fx = ClusterFixture::new(&[]);
        assert_eq!(unsafe { syn_scl_name2id(b"nope") }, 0);
    }

    /// The id is the cluster's index OFFSET past `SYNID_CLUSTER`, so
    /// even the first cluster never collides with a real highlight
    /// group id.
    #[test]
    fn syn_scl_name2id_offsets_the_index_past_the_cluster_base() {
        let _lock = crate::globals::global_state_test_lock();
        let _fx = ClusterFixture::new(&[b"first", b"second"]);

        assert_eq!(unsafe { syn_scl_name2id(b"first") }, SYNID_CLUSTER);
        assert_eq!(unsafe { syn_scl_name2id(b"second") }, SYNID_CLUSTER + 1);
        assert_eq!(unsafe { syn_scl_name2id(b"missing") }, 0);
    }

    /// Matching is case-INSENSITIVE, via each cluster's precomputed
    /// uppercase name.
    #[test]
    fn syn_scl_name2id_ignores_case() {
        let _lock = crate::globals::global_state_test_lock();
        let _fx = ClusterFixture::new(&[b"myCluster"]);

        assert_eq!(unsafe { syn_scl_name2id(b"MYCLUSTER") }, SYNID_CLUSTER);
        assert_eq!(unsafe { syn_scl_name2id(b"mycluster") }, SYNID_CLUSTER);
        assert_eq!(unsafe { syn_scl_name2id(b"MyCluster") }, SYNID_CLUSTER);
    }

    /// The scan runs BACKWARDS, so with duplicate names the LAST
    /// entry wins. A forward scan would return the first.
    #[test]
    fn syn_scl_name2id_prefers_the_last_of_two_duplicate_names() {
        let _lock = crate::globals::global_state_test_lock();
        let _fx = ClusterFixture::new(&[b"dup", b"other", b"dup"]);
        assert_eq!(unsafe { syn_scl_name2id(b"dup") }, SYNID_CLUSTER + 2);
    }

    /// A cluster whose name was cleared is skipped rather than
    /// matching an empty needle.
    #[test]
    fn syn_scl_name2id_skips_a_cleared_cluster() {
        let _lock = crate::globals::global_state_test_lock();
        let fx = ClusterFixture::new(&[b"alpha", b"beta"]);
        syn_clear_cluster(&mut fx.block_mut().b_syn_clusters.items[1]);

        assert_eq!(unsafe { syn_scl_name2id(b"alpha") }, SYNID_CLUSTER);
        assert_eq!(unsafe { syn_scl_name2id(b"beta") }, 0);
    }

    /// The length-limited form stops at `len`, ignoring whatever
    /// follows in the buffer.
    #[test]
    fn syn_scl_namen2id_uses_only_the_first_len_bytes() {
        let _lock = crate::globals::global_state_test_lock();
        let _fx = ClusterFixture::new(&[b"abc"]);

        assert_eq!(unsafe { syn_scl_namen2id(b"abcdef", 3) }, SYNID_CLUSTER);
        assert_eq!(unsafe { syn_scl_namen2id(b"abcdef", 6) }, 0);
    }

    #[test]
    fn synid_constants_match_the_original() {
        assert_eq!((SYNID_TOP, SYNID_CONTAINED, SYNID_CLUSTER), (21000, 22000, 23000));
        assert_eq!(MAX_CLUSTER_ID, 32767 - 23000);
    }

    // ---- SynClusterT / syn_clear_cluster ----

    /// The original frees only the cluster's MEMBERS, leaving the
    /// entry itself in the table for the caller to remove or reuse -
    /// so this is not a removal, and the slot must survive.
    #[test]
    fn syn_clear_cluster_releases_the_members_but_keeps_the_entry() {
        let mut block = crate::buffer_defs::SynblockT::default();
        block.b_syn_clusters.items = vec![
            SynClusterT {
                scl_name: Some(b"myCluster".to_vec()),
                scl_name_u: Some(b"MYCLUSTER".to_vec()),
                scl_list: vec![3, 7, 11],
            },
            SynClusterT {
                scl_name: Some(b"other".to_vec()),
                scl_name_u: Some(b"OTHER".to_vec()),
                scl_list: vec![1],
            },
        ];

        syn_clear_cluster(&mut block.b_syn_clusters.items[0]);

        assert_eq!(
            block.b_syn_clusters.ga_len(),
            2,
            "the entry stays in the table"
        );
        let cleared = &block.b_syn_clusters.items[0];
        assert_eq!(cleared.scl_name, None);
        assert_eq!(cleared.scl_name_u, None);
        assert!(cleared.scl_list.is_empty());

        // The neighbouring cluster must be untouched.
        let kept = &block.b_syn_clusters.items[1];
        assert_eq!(kept.scl_name, Some(b"other".to_vec()));
        assert_eq!(kept.scl_list, vec![1]);
    }

    #[test]
    fn syn_clear_cluster_is_safe_to_repeat() {
        let mut c = SynClusterT {
            scl_name: Some(b"x".to_vec()),
            scl_name_u: Some(b"X".to_vec()),
            scl_list: vec![1, 2],
        };
        syn_clear_cluster(&mut c);
        syn_clear_cluster(&mut c);
        assert_eq!(c.scl_name, None);
        assert!(c.scl_list.is_empty());
    }

    #[test]
    fn syn_clusters_start_empty() {
        let block = crate::buffer_defs::SynblockT::default();
        assert!(block.b_syn_clusters.is_empty());
        assert_eq!(block.b_syn_clusters.ga_len(), 0);
    }

    // ---- get_group_name ----

    #[test]
    fn get_group_name_splits_the_name_from_the_rest() {
        let arg = b"myGroup keyword foo";
        let (name_end, rest) = get_group_name(arg).expect("two arguments given");
        assert_eq!(&arg[..name_end], b"myGroup");
        assert_eq!(&arg[rest..], b"keyword foo");
    }

    #[test]
    fn get_group_name_skips_extra_whitespace_before_the_rest() {
        let arg = b"myGroup   \t keyword";
        let (name_end, rest) = get_group_name(arg).expect("two arguments given");
        assert_eq!(&arg[..name_end], b"myGroup");
        assert_eq!(&arg[rest..], b"keyword");
    }

    /// A name with nothing after it is not enough arguments.
    #[test]
    fn get_group_name_rejects_a_lone_group_name() {
        assert_eq!(get_group_name(b"myGroup"), None);
        assert_eq!(get_group_name(b"myGroup   "), None);
    }

    /// An argument that starts by ending the command has no name at
    /// all.
    #[test]
    fn get_group_name_rejects_an_empty_argument() {
        assert_eq!(get_group_name(b""), None);
        assert_eq!(get_group_name(b"\" a comment"), None);
    }

    /// A `|` is a legitimate part of a syntax PATTERN, so it must not
    /// terminate the scan even though it ends an Ex command
    /// elsewhere. The original checks the rest for NUL specifically
    /// for this reason; using `ends_excmd` on the rest instead would
    /// wrongly reject this.
    #[test]
    fn get_group_name_accepts_a_pattern_containing_a_bar() {
        let arg = b"myGroup /a\\|b/";
        let (name_end, rest) = get_group_name(arg).expect("a bar is part of the pattern");
        assert_eq!(&arg[..name_end], b"myGroup");
        assert_eq!(&arg[rest..], b"/a\\|b/");
    }

    /// ...but a `|` as the very FIRST character still ends the
    /// command, since that check is on `arg` rather than on the rest.
    #[test]
    fn get_group_name_rejects_a_leading_bar() {
        assert_eq!(get_group_name(b"| next"), None);
    }

    fn lpos(lnum: crate::pos_defs::LinenrT, col: crate::pos_defs::ColnrT) -> LposT {
        LposT { lnum, col }
    }

    /// A position on a later line is pulled back WHOLESALE - the line
    /// number moves too, not just the column.
    #[test]
    fn limit_pos_pulls_back_a_position_on_a_later_line() {
        let mut pos = lpos(10, 3);
        limit_pos(&mut pos, &lpos(5, 7));
        assert_eq!((pos.lnum, pos.col), (5, 7));
    }

    /// On the SAME line only the column is clamped.
    #[test]
    fn limit_pos_clamps_only_the_column_on_the_same_line() {
        let mut pos = lpos(5, 20);
        limit_pos(&mut pos, &lpos(5, 7));
        assert_eq!((pos.lnum, pos.col), (5, 7));
    }

    /// An EARLIER line is left alone even when its column exceeds the
    /// limit's, because a column only orders positions within one
    /// line. An implementation clamping the column unconditionally
    /// would wrongly move this position.
    #[test]
    fn limit_pos_leaves_an_earlier_line_alone_even_with_a_larger_column() {
        let mut pos = lpos(2, 99);
        limit_pos(&mut pos, &lpos(5, 7));
        assert_eq!((pos.lnum, pos.col), (2, 99));
    }

    /// A position already within the limit is untouched.
    #[test]
    fn limit_pos_leaves_a_position_within_the_limit_alone() {
        let mut pos = lpos(5, 3);
        limit_pos(&mut pos, &lpos(5, 7));
        assert_eq!((pos.lnum, pos.col), (5, 3));

        let mut same = lpos(5, 7);
        limit_pos(&mut same, &lpos(5, 7));
        assert_eq!((same.lnum, same.col), (5, 7));
    }

    #[test]
    fn syn_compare_stub_orders_ascending() {
        assert!(syn_compare_stub(1, 2) < 0);
        assert!(syn_compare_stub(2, 1) > 0);
        assert_eq!(syn_compare_stub(3, 3), 0);
        // Negative ids must not confuse the comparison.
        assert!(syn_compare_stub(-5, 1) < 0);
        assert!(syn_compare_stub(1, -5) > 0);
    }

    #[test]
    fn syn_compare_stub_sorts_a_list_ascending() {
        let mut v: [i16; 4] = [30, -1, 10, 20];
        v.sort_by(|a, b| syn_compare_stub(*a, *b).cmp(&0));
        assert_eq!(v, [-1, 10, 20, 30]);
    }
}
