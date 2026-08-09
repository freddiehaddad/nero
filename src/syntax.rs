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

/// One syntax cluster - a named set of syntax group ids
/// (`syn_cluster_T`).
///
/// All three fields are owned by the original (each is `xfree`d in
/// `syn_clear_cluster`), so they become owned Rust values.
/// `scl_list` is a plain `Vec` rather than the original's
/// separately-counted `int16_t *`, since a `Vec` carries its own
/// length.
#[derive(Debug, Clone, Default)]
pub struct SynClusterT {
    /// syntax cluster name (`scl_name`).
    pub scl_name: Option<Vec<u8>>,
    /// uppercase of `scl_name` (`scl_name_u`).
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

    // ---- syn_scl_name2id / syn_scl_namen2id ----

    /// Installs a syntax block with the given cluster names as
    /// `curwin->w_s`, restoring `curwin` on drop even through a panic.
    struct SynBlockGuard {
        prev: *mut crate::buffer_defs::WinT,
    }

    impl SynBlockGuard {
        fn install(win: *mut crate::buffer_defs::WinT) -> Self {
            let g = unsafe { crate::globals::GLOBALS.get_mut() };
            let me = Self { prev: g.curwin };
            g.curwin = win;
            me
        }
    }

    impl Drop for SynBlockGuard {
        fn drop(&mut self) {
            unsafe { crate::globals::GLOBALS.get_mut() }.curwin = self.prev;
        }
    }

    /// Builds a window whose syntax block holds clusters with the
    /// given names, each with its uppercase form precomputed exactly
    /// as the real cluster-creation path does.
    fn win_with_clusters(
        names: &[&[u8]],
    ) -> (Box<crate::buffer_defs::WinT>, Box<crate::buffer_defs::SynblockT>) {
        let mut block = Box::new(crate::buffer_defs::SynblockT::default());
        block.b_syn_clusters.items = names
            .iter()
            .map(|n| SynClusterT {
                scl_name: Some((*n).to_vec()),
                scl_name_u: Some(crate::strings::vim_strsave_up(n)),
                scl_list: Vec::new(),
            })
            .collect();
        let mut win = Box::new(crate::buffer_defs::WinT::default());
        win.w_s = std::ptr::from_mut(&mut *block);
        (win, block)
    }

    #[test]
    fn syn_scl_name2id_returns_zero_when_there_are_no_clusters() {
        let _lock = crate::globals::global_state_test_lock();
        let (mut win, _block) = win_with_clusters(&[]);
        let _g = SynBlockGuard::install(std::ptr::from_mut(&mut *win));
        assert_eq!(unsafe { syn_scl_name2id(b"nope") }, 0);
    }

    /// The id is the cluster's index OFFSET past `SYNID_CLUSTER`, so
    /// even the first cluster never collides with a real highlight
    /// group id.
    #[test]
    fn syn_scl_name2id_offsets_the_index_past_the_cluster_base() {
        let _lock = crate::globals::global_state_test_lock();
        let (mut win, _block) = win_with_clusters(&[b"first", b"second"]);
        let _g = SynBlockGuard::install(std::ptr::from_mut(&mut *win));

        assert_eq!(unsafe { syn_scl_name2id(b"first") }, SYNID_CLUSTER);
        assert_eq!(unsafe { syn_scl_name2id(b"second") }, SYNID_CLUSTER + 1);
        assert_eq!(unsafe { syn_scl_name2id(b"missing") }, 0);
    }

    /// Matching is case-INSENSITIVE, via each cluster's precomputed
    /// uppercase name.
    #[test]
    fn syn_scl_name2id_ignores_case() {
        let _lock = crate::globals::global_state_test_lock();
        let (mut win, _block) = win_with_clusters(&[b"myCluster"]);
        let _g = SynBlockGuard::install(std::ptr::from_mut(&mut *win));

        assert_eq!(unsafe { syn_scl_name2id(b"MYCLUSTER") }, SYNID_CLUSTER);
        assert_eq!(unsafe { syn_scl_name2id(b"mycluster") }, SYNID_CLUSTER);
        assert_eq!(unsafe { syn_scl_name2id(b"MyCluster") }, SYNID_CLUSTER);
    }

    /// The scan runs BACKWARDS, so with duplicate names the LAST
    /// entry wins. A forward scan would return the first.
    #[test]
    fn syn_scl_name2id_prefers_the_last_of_two_duplicate_names() {
        let _lock = crate::globals::global_state_test_lock();
        let (mut win, _block) = win_with_clusters(&[b"dup", b"other", b"dup"]);
        let _g = SynBlockGuard::install(std::ptr::from_mut(&mut *win));
        assert_eq!(unsafe { syn_scl_name2id(b"dup") }, SYNID_CLUSTER + 2);
    }

    /// A cluster whose name was cleared is skipped rather than
    /// matching an empty needle.
    #[test]
    fn syn_scl_name2id_skips_a_cleared_cluster() {
        let _lock = crate::globals::global_state_test_lock();
        let (mut win, mut block) = win_with_clusters(&[b"alpha", b"beta"]);
        syn_clear_cluster(&mut block.b_syn_clusters.items[1]);
        win.w_s = std::ptr::from_mut(&mut *block);
        let _g = SynBlockGuard::install(std::ptr::from_mut(&mut *win));

        assert_eq!(unsafe { syn_scl_name2id(b"alpha") }, SYNID_CLUSTER);
        assert_eq!(unsafe { syn_scl_name2id(b"beta") }, 0);
    }

    /// The length-limited form stops at `len`, ignoring whatever
    /// follows in the buffer.
    #[test]
    fn syn_scl_namen2id_uses_only_the_first_len_bytes() {
        let _lock = crate::globals::global_state_test_lock();
        let (mut win, _block) = win_with_clusters(&[b"abc"]);
        let _g = SynBlockGuard::install(std::ptr::from_mut(&mut *win));

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
