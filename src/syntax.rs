//! Translated from `src/nvim/syntax.c` (tractable core only).
//!
//! `syntax.c` (~7500 lines) is neovim's syntax-highlighting engine:
//! the `:syntax` command family, the pattern/keyword/cluster tables,
//! and the per-line state machine that drives highlighting. Almost
//! every function depends on `synstate_T`/`stateitem_T`/`synpat_T`
//! and the regex engine (`regprog_T`/`reg_extmatch_T`), none of which
//! are translated.
//!
//! Translated: [`limit_pos`] and [`syn_compare_stub`] - two small,
//! self-contained helpers with no design freedom of their own,
//! needing only the already-real [`crate::pos_defs::LposT`]. Both are
//! translated ahead of their real callers (`syn_add_end_off`/
//! `syn_add_start_off` and the cluster-list sort respectively, none
//! translated), matching this crate's established "translate a small,
//! mechanically-correct piece ahead of the surrounding engine"
//! precedent (e.g. `drawline.rs`'s `get_lcs_ext`).
//!
//! Deferred: everything else in the file.

use crate::pos_defs::LposT;

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
