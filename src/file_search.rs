//! Translated from `src/nvim/file_search.c` (tractable core only).
//!
//! `file_search.c` implements the `findfile()`/`:find` file-searching
//! engine - almost entirely dependent on the full `vim_findfile`
//! search-context/path-traversal machinery, none of which is
//! translated.
//!
//! Translated: [`vim_findfile_stopdir`], [`ff_wc_equal`],
//! [`ff_path_in_stoplist`] - small, pure string/path helpers needing
//! only already-translated pieces (`mbyte.rs`, `path.rs`,
//! `option_vars.rs`).
//!
//! Deferred: everything else - `vim_findfile_init`/`vim_findfile`/
//! `find_file_in_path`/`find_directory_in_path`/`grab_file_name`/
//! `file_name_in_line`/`vim_chdirfile`/`vim_chdir`/
//! `find_file_in_path_option`/`find_file_name_in_path`/
//! `file_name_at_cursor`, all needing the full search-context/path-
//! traversal/file-expansion machinery.

use crate::mbyte::utf_ptr2char;
use crate::path::{path_fnamencmp, vim_ispathsep};

/// Shared filename-expansion buffer (`ff_expand_buffer`).
static FF_EXPAND_BUFFER: crate::globals::GlobalCell<Option<Vec<u8>>> =
    crate::globals::GlobalCell::new(None);

/// One visited file or directory (`ff_visited_T`).
#[allow(dead_code)]
#[derive(Debug, Default)]
struct FfVisitedT {
    ffv_next: Option<Box<FfVisitedT>>,
    ffv_wc_path: Option<Vec<u8>>,
    file_id_valid: bool,
    file_id: crate::os::fs_defs::FileID,
    ffv_fname: Vec<u8>,
}

/// Free an entire visited-file chain (`ff_free_visited_list`).
#[allow(dead_code)]
fn ff_free_visited_list(list: &mut Option<Box<FfVisitedT>>) {
    *list = None;
}

/// Release shared find-file expansion storage (`free_findfile`).
///
/// # Safety
/// Must not run concurrently with file-search operations.
pub unsafe fn free_findfile() {
    // SAFETY: forwarded from this function's own safety doc.
    *unsafe { FF_EXPAND_BUFFER.get_mut() } = None;
}

/// Splits `buf` at the first unescaped `;` (where `\;` is an escape
/// sequence for a literal `;`), returning the de-escaped stopdir list
/// before it and the remainder after it (`vim_findfile_stopdir`).
///
/// Returns `(stopdir, rest)`: `stopdir` is `buf` up to (but not
/// including) the terminating `;`, with any `\;` sequences un-escaped
/// to a plain `;`; `rest` is `None` if there is no unescaped `;` (this
/// is the last/only stopdir segment), or `Some(...)` with everything
/// after it otherwise.
///
/// The original mutates `buf` in place (de-escaping into the same
/// allocation - always safe since de-escaping only ever shrinks or
/// preserves length) and returns a pointer into that same buffer for
/// the remainder. This instead returns a freshly-owned `Vec<u8>` for
/// the de-escaped segment and a borrowed sub-slice of `buf` for the
/// remainder - nothing here needs the two results to share the
/// original's single backing allocation.
#[must_use]
pub fn vim_findfile_stopdir(buf: &[u8]) -> (Vec<u8>, Option<&[u8]>) {
    let mut out = Vec::with_capacity(buf.len());
    let mut i = 0;
    while i < buf.len() {
        if buf[i] == b';' {
            return (out, Some(&buf[i + 1..]));
        }
        if buf[i] == b'\\' && buf.get(i + 1) == Some(&b';') {
            out.push(b';');
            i += 2;
        } else {
            out.push(buf[i]);
            i += 1;
        }
    }
    (out, None)
}

/// Checks if two wildcard paths are equal (`ff_wc_equal`). They are
/// equal if they have the same length, compare equal character by
/// character (respecting `'fileignorecase'`), and the only difference
/// (if any) is the single byte right after a `**` - the internal
/// recursion-depth count, stored as one raw byte whose *value* is the
/// count (`"**3"` becomes the 3 bytes `**` + `0x03`, not ASCII digit
/// characters - see `file_search.c`'s own comment on this encoding).
///
/// `None` represents the original's `NULL` (both `None` is equal,
/// exactly one `None` is never equal to a real path).
///
/// The original's `s1 == s2` pointer-identity fast path is omitted -
/// it's a pure performance micro-optimization for "the exact same
/// pointer was passed twice", not a behavior difference: the
/// character-by-character comparison below already returns `true` for
/// identical content regardless.
#[must_use]
pub fn ff_wc_equal(s1: Option<&[u8]>, s2: Option<&[u8]>) -> bool {
    let (Some(s1), Some(s2)) = (s1, s2) else {
        return s1.is_none() && s2.is_none();
    };

    let fic = unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_fic != 0;
    let mut prev1 = 0i32;
    let mut prev2 = 0i32;
    let mut i = 0;
    let mut j = 0;
    while i < s1.len() && j < s2.len() {
        let c1 = utf_ptr2char(&s1[i..]);
        let c2 = utf_ptr2char(&s2[j..]);

        let differs = if fic {
            // SAFETY: mb_tolower has no preconditions beyond a valid
            // codepoint-or-negative value, which utf_ptr2char always
            // returns.
            unsafe { crate::mbyte::mb_tolower(c1) != crate::mbyte::mb_tolower(c2) }
        } else {
            c1 != c2
        };
        if differs && (prev1 != i32::from(b'*') || prev2 != i32::from(b'*')) {
            return false;
        }
        prev2 = prev1;
        prev1 = c1;

        // SAFETY: `s1[i..]`/`s2[j..]` are valid, non-empty (loop
        // condition) byte slices.
        i += unsafe { crate::mbyte::utfc_ptr2len(&s1[i..]) } as usize;
        j += unsafe { crate::mbyte::utfc_ptr2len(&s2[j..]) } as usize;
    }
    i == s1.len() && j == s2.len()
}

/// Whether `path` is one of `stopdirs`, or an ANCESTOR of one, that a
/// path search should not recurse past (`ff_path_in_stoplist`).
///
/// Note the direction: since the underlying `path_fnamencmp` comparison
/// stops as soon as either string ends, a match requires `path` (the
/// shorter side) to be a byte-for-byte prefix of a `stopdirs` entry
/// (the longer side) at a path-separator boundary - so `"/home"`
/// matches a stopdir of `"/home/rks"`, not the other way around.
#[must_use]
pub fn ff_path_in_stoplist(path: &[u8], stopdirs: &[&[u8]]) -> bool {
    // eat up trailing path separators, except the first
    let mut path_len = path.len();
    while path_len > 1 && vim_ispathsep(i32::from(path[path_len - 1])) {
        path_len -= 1;
    }
    let path = &path[..path_len];

    // if no path consider it as match
    if path.is_empty() {
        return true;
    }

    for &stopdir in stopdirs {
        // match for parent directory. So '/home' also matches
        // '/home/rks'. Check for a path separator in stopdir, else
        // '/home/r' would also match '/home/rks'.
        // SAFETY: both are plain, valid byte slices.
        if unsafe { path_fnamencmp(stopdir, path, path.len()) } == 0
            && (stopdir.len() <= path.len() || vim_ispathsep(i32::from(stopdir[path.len()])))
        {
            return true;
        }
    }
    false
}

/// One entry of the directory search stack (`ff_stack_T`).
///
/// The original's two `String` fields become owned `Vec<u8>`, and the
/// `char **` file array becomes a `Vec<Vec<u8>>`, so
/// `ffs_filearray_size` is dropped - the `Vec` carries its own length.
/// `ffs_filearray_cur` stays, since it is a cursor rather than a
/// length.
///
/// `ffs_prev` remains a raw pointer: the stack is threaded through the
/// nodes themselves, and the search context owns them.
#[derive(Debug, Default)]
pub struct FfStackT {
    /// the entry below this one on the stack (`ffs_prev`).
    pub ffs_prev: *mut FfStackT,
    /// the wildcard-free part of the search path (`ffs_fix_path`).
    pub ffs_fix_path: Vec<u8>,
    /// the part of the search path holding wildcards (`ffs_wc_path`).
    pub ffs_wc_path: Vec<u8>,
    /// entries found in `ffs_fix_path`, matched by the first wildcard
    /// of the wildcard part (`ffs_filearray`).
    pub ffs_filearray: Vec<Vec<u8>>,
    /// how far through `ffs_filearray` a partly handled directory got
    /// (`ffs_filearray_cur`).
    pub ffs_filearray_cur: i32,
    /// `0` the first time this directory is worked on, `1` when it was
    /// already partly searched in an earlier step (`ffs_stage`).
    pub ffs_stage: i32,
    /// depth in the directory tree, counting back from the level given
    /// to `vim_findfile_init` (`ffs_level`).
    pub ffs_level: i32,
    /// whether `"**"` was already expanded to an empty string
    /// (`ffs_star_star_empty`).
    pub ffs_star_star_empty: i32,
}

/// Builds a stack entry from the two halves of a search path
/// (`ff_create_stack_element`).
///
/// Both halves are optional: the original substitutes an empty string
/// for a null one, noting this saves null checks in `vim_findfile`.
/// `None` gets the same treatment here.
///
/// The returned node is owned by the caller and must eventually be
/// released with [`ff_free_stack_element`].
#[must_use]
pub fn ff_create_stack_element(
    fix_part: Option<&[u8]>,
    wc_part: Option<&[u8]>,
    level: i32,
    star_star_empty: i32,
) -> *mut FfStackT {
    Box::into_raw(Box::new(FfStackT {
        ffs_prev: std::ptr::null_mut(),
        ffs_fix_path: fix_part.unwrap_or(b"").to_vec(),
        ffs_wc_path: wc_part.unwrap_or(b"").to_vec(),
        ffs_filearray: Vec::new(),
        ffs_filearray_cur: 0,
        ffs_stage: 0,
        ffs_level: level,
        ffs_star_star_empty: star_star_empty,
    }))
}

/// Pushes a directory onto the search stack (`ff_push`).
///
/// The original threads the whole `ff_search_ctx_T` through, but only
/// ever touches its `ffsc_stack_ptr`, so this takes that head pointer
/// directly - the same treatment the other helpers here already give
/// their arguments.
///
/// A null `stack_ptr` is ignored, as in the original, which notes this
/// guards against a crash rather than reporting an error.
///
/// # Safety
/// `stack_ptr` must be null or point to a node that is not already on
/// this stack, and `head` must be the live head of that stack.
pub unsafe fn ff_push(head: &mut *mut FfStackT, stack_ptr: *mut FfStackT) {
    if stack_ptr.is_null() {
        return;
    }
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { (*stack_ptr).ffs_prev = *head };
    *head = stack_ptr;
}

/// Pops a directory off the search stack, or returns null when it is
/// empty (`ff_pop`).
///
/// Ownership of the returned node passes to the caller, which is
/// expected to release it with [`ff_free_stack_element`].
///
/// # Safety
/// `head` must be the live head of a stack whose nodes satisfy
/// [`ff_push`]'s contract.
pub unsafe fn ff_pop(head: &mut *mut FfStackT) -> *mut FfStackT {
    let sptr = *head;
    if !sptr.is_null() {
        // SAFETY: forwarded from this function's own safety doc.
        *head = unsafe { (*sptr).ffs_prev };
    }
    sptr
}

/// Releases one stack entry (`ff_free_stack_element`).
///
/// The original frees the two path strings and the file array by hand;
/// all three are owned here, so reconstructing the `Box` releases
/// them along with the node.
///
/// # Safety
/// `stack_ptr` must be null or a node from [`Box::into_raw`] that is
/// no longer on any stack, and no other pointer to it may be used
/// afterwards.
pub unsafe fn ff_free_stack_element(stack_ptr: *mut FfStackT) {
    if stack_ptr.is_null() {
        return;
    }
    // SAFETY: forwarded from this function's own safety doc.
    drop(unsafe { Box::from_raw(stack_ptr) });
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ExpandBufferGuard(Option<Vec<u8>>);

    impl ExpandBufferGuard {
        fn install(value: Option<Vec<u8>>) -> Self {
            Self(std::mem::replace(
                unsafe { FF_EXPAND_BUFFER.get_mut() },
                value,
            ))
        }
    }

    impl Drop for ExpandBufferGuard {
        fn drop(&mut self) {
            *unsafe { FF_EXPAND_BUFFER.get_mut() } = self.0.take();
        }
    }

    #[test]
    fn free_findfile_clears_allocated_expansion_storage() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard =
            ExpandBufferGuard::install(Some(vec![
                0;
                crate::os::os_defs::MAXPATHL as usize
            ]));
        unsafe { free_findfile() };
        assert!(unsafe { FF_EXPAND_BUFFER.get_mut() }.is_none());
    }

    #[test]
    fn visited_file_node_owns_path_identity_and_next_link() {
        let node = FfVisitedT {
            ffv_next: Some(Box::new(FfVisitedT {
                ffv_fname: b"second".to_vec(),
                ..Default::default()
            })),
            ffv_wc_path: Some(b"**/*.rs".to_vec()),
            file_id_valid: true,
            file_id: crate::os::fs_defs::FileID {
                inode: 7,
                device_id: 9,
            },
            ffv_fname: b"first".to_vec(),
        };
        assert_eq!(node.ffv_fname, b"first");
        assert_eq!(node.ffv_wc_path.as_deref(), Some(b"**/*.rs".as_slice()));
        assert!(node.file_id_valid);
        assert_eq!(node.file_id.inode, 7);
        assert_eq!(
            node.ffv_next.as_deref().map(|next| next.ffv_fname.as_slice()),
            Some(b"second".as_slice())
        );
    }

    #[test]
    fn ff_free_visited_list_drops_the_entire_owned_chain() {
        let mut list = Some(Box::new(FfVisitedT {
            ffv_next: Some(Box::new(FfVisitedT {
                ffv_fname: b"tail".to_vec(),
                ..Default::default()
            })),
            ffv_wc_path: Some(b"*.vim".to_vec()),
            ffv_fname: b"head".to_vec(),
            ..Default::default()
        }));
        ff_free_visited_list(&mut list);
        assert!(list.is_none());
    }

    // ---- ff_create_stack_element ----

    /// Both halves are stored as given, and every other field starts
    /// cleared.
    #[test]
    fn ff_create_stack_element_stores_both_path_halves() {
        let node = ff_create_stack_element(Some(b"/usr/share"), Some(b"**/doc"), 4, 1);
        unsafe {
            assert_eq!((*node).ffs_fix_path, b"/usr/share".to_vec());
            assert_eq!((*node).ffs_wc_path, b"**/doc".to_vec());
            assert_eq!((*node).ffs_level, 4);
            assert_eq!((*node).ffs_star_star_empty, 1);
            assert!((*node).ffs_prev.is_null());
            assert!((*node).ffs_filearray.is_empty());
            assert_eq!((*node).ffs_filearray_cur, 0);
            assert_eq!((*node).ffs_stage, 0);
            ff_free_stack_element(node);
        }
    }

    /// A missing half becomes an empty string rather than staying
    /// absent, which is what lets the search loop skip null checks.
    #[test]
    fn ff_create_stack_element_substitutes_empty_paths() {
        let node = ff_create_stack_element(None, None, 0, 0);
        unsafe {
            assert!((*node).ffs_fix_path.is_empty());
            assert!((*node).ffs_wc_path.is_empty());
            ff_free_stack_element(node);
        }
    }

    /// A fresh entry is not linked to anything, so it can be pushed
    /// straight onto a stack.
    #[test]
    fn ff_create_stack_element_produces_a_pushable_entry() {
        let mut head: *mut FfStackT = std::ptr::null_mut();
        unsafe {
            ff_push(&mut head, ff_create_stack_element(Some(b"a"), None, 1, 0));
            ff_push(&mut head, ff_create_stack_element(Some(b"b"), None, 2, 0));

            assert_eq!((*head).ffs_fix_path, b"b".to_vec());
            ff_free_stack_element(ff_pop(&mut head));
            assert_eq!((*head).ffs_fix_path, b"a".to_vec());
            ff_free_stack_element(ff_pop(&mut head));
        }
        assert!(head.is_null());
    }

    // ---- ff_push / ff_pop / ff_free_stack_element ----

    fn stack_node(level: i32) -> *mut FfStackT {
        Box::into_raw(Box::new(FfStackT {
            ffs_fix_path: b"/tmp".to_vec(),
            ffs_wc_path: b"**".to_vec(),
            ffs_filearray: vec![b"a".to_vec(), b"b".to_vec()],
            ffs_level: level,
            ..Default::default()
        }))
    }

    #[test]
    fn ff_pop_returns_null_for_an_empty_stack() {
        let mut head: *mut FfStackT = std::ptr::null_mut();
        assert!(unsafe { ff_pop(&mut head) }.is_null());
        assert!(head.is_null(), "the head must stay empty");
    }

    /// Pushing null is ignored rather than corrupting the stack.
    #[test]
    fn ff_push_ignores_a_null_entry() {
        let mut head = stack_node(1);
        unsafe { ff_push(&mut head, std::ptr::null_mut()) };

        assert_eq!(unsafe { (*head).ffs_level }, 1, "the head is unchanged");
        unsafe { ff_free_stack_element(ff_pop(&mut head)) };
    }

    /// The stack is LIFO, so entries come back in reverse order and
    /// the head empties out exactly once.
    #[test]
    fn ff_push_and_pop_are_last_in_first_out() {
        let mut head: *mut FfStackT = std::ptr::null_mut();
        unsafe {
            ff_push(&mut head, stack_node(1));
            ff_push(&mut head, stack_node(2));
            ff_push(&mut head, stack_node(3));
        }

        let mut levels = Vec::new();
        loop {
            let node = unsafe { ff_pop(&mut head) };
            if node.is_null() {
                break;
            }
            levels.push(unsafe { (*node).ffs_level });
            unsafe { ff_free_stack_element(node) };
        }

        assert_eq!(levels, vec![3, 2, 1]);
        assert!(head.is_null());
    }

    /// Popping hands the entry over intact, so the caller can still
    /// read it before releasing it.
    #[test]
    fn ff_pop_preserves_the_entrys_contents() {
        let mut head: *mut FfStackT = std::ptr::null_mut();
        unsafe { ff_push(&mut head, stack_node(7)) };

        let node = unsafe { ff_pop(&mut head) };
        assert!(!node.is_null());
        unsafe {
            assert_eq!((*node).ffs_fix_path, b"/tmp".to_vec());
            assert_eq!((*node).ffs_wc_path, b"**".to_vec());
            assert_eq!((*node).ffs_filearray.len(), 2);
            assert_eq!((*node).ffs_level, 7);
            ff_free_stack_element(node);
        }
    }

    #[test]
    fn ff_free_stack_element_accepts_null() {
        unsafe { ff_free_stack_element(std::ptr::null_mut()) };
    }

    #[test]
    fn vim_findfile_stopdir_splits_at_semicolon() {
        let (stopdir, rest) = vim_findfile_stopdir(b"/home;/usr");
        assert_eq!(stopdir, b"/home");
        assert_eq!(rest, Some(b"/usr".as_slice()));
    }

    #[test]
    fn vim_findfile_stopdir_no_semicolon_is_the_whole_thing() {
        let (stopdir, rest) = vim_findfile_stopdir(b"/home");
        assert_eq!(stopdir, b"/home");
        assert_eq!(rest, None);
    }

    #[test]
    fn vim_findfile_stopdir_unescapes_backslash_semicolon() {
        let (stopdir, rest) = vim_findfile_stopdir(b"/a\\;b;/rest");
        assert_eq!(stopdir, b"/a;b");
        assert_eq!(rest, Some(b"/rest".as_slice()));
    }

    #[test]
    fn vim_findfile_stopdir_trailing_backslash_is_safe() {
        // A lone trailing backslash: must not panic (the one-past-the-
        // end read this function's own doc discusses).
        let (stopdir, rest) = vim_findfile_stopdir(b"/home\\");
        assert_eq!(stopdir, b"/home\\");
        assert_eq!(rest, None);
    }

    #[test]
    fn vim_findfile_stopdir_empty_input() {
        let (stopdir, rest) = vim_findfile_stopdir(b"");
        assert_eq!(stopdir, b"");
        assert_eq!(rest, None);
    }

    #[test]
    fn ff_wc_equal_both_none_is_equal() {
        assert!(ff_wc_equal(None, None));
    }

    #[test]
    fn ff_wc_equal_one_none_is_not_equal() {
        assert!(!ff_wc_equal(None, Some(b"a")));
        assert!(!ff_wc_equal(Some(b"a"), None));
    }

    #[test]
    fn ff_wc_equal_identical_strings() {
        assert!(ff_wc_equal(Some(b"foo/bar"), Some(b"foo/bar")));
    }

    #[test]
    fn ff_wc_equal_different_strings() {
        assert!(!ff_wc_equal(Some(b"foo"), Some(b"bar")));
    }

    #[test]
    fn ff_wc_equal_different_lengths() {
        assert!(!ff_wc_equal(Some(b"foo"), Some(b"foobar")));
    }

    #[test]
    fn ff_wc_equal_star_counters_are_equal() {
        // The '**N' recursion-depth count is internally stored as a
        // single RAW BYTE whose VALUE is the count (see file_search.c's
        // own comment: "'**3' is transposed to '**^C'... '**76' is
        // transposed to '**N'"), not ASCII digit characters - so "**\20"
        // in ff_wc_equal's own doc comment means the 3 bytes [b'*',
        // b'*', 20u8], not 5 literal characters. Differing counter
        // bytes should be ignored since both preceding characters are
        // '*'.
        assert!(ff_wc_equal(Some(&[b'*', b'*', 20]), Some(&[b'*', b'*', 24])));
    }

    #[test]
    fn ff_wc_equal_only_exempts_the_position_right_after_double_star() {
        // A literal backslash-then-digit-characters sequence (NOT the
        // internal raw-byte encoding above) does NOT get the exemption
        // for its second digit, since prev1 is the first digit (not
        // '*') by the time the second digit is compared.
        assert!(!ff_wc_equal(Some(b"**\\20"), Some(b"**\\24")));
    }

    #[test]
    fn ff_path_in_stoplist_empty_path_matches() {
        assert!(ff_path_in_stoplist(b"", &[b"/home"]));
    }

    #[test]
    fn ff_path_in_stoplist_exact_match() {
        assert!(ff_path_in_stoplist(b"/home", &[b"/home"]));
    }

    #[test]
    fn ff_path_in_stoplist_parent_directory_matches() {
        // path_fnamencmp(stopdir, path, path.len()) stops as soon as
        // EITHER string ends, so a match requires `path` (the shorter
        // side) to be a byte-for-byte prefix of `stopdir` (the longer
        // side) - i.e. `path` is an ANCESTOR of one of the stopdirs,
        // not the other way around. Verified by direct derivation
        // against path_fnamencmp's own already-tested short-circuit-at-
        // NUL behavior before writing this assertion.
        assert!(ff_path_in_stoplist(b"/home", &[b"/home/rks"]));
    }

    #[test]
    fn ff_path_in_stoplist_prefix_without_path_sep_does_not_match() {
        // '/home/r' should NOT match a '/home/rks' stopdir - it's just
        // a string prefix, not a real ancestor directory (no path
        // separator right after it in the stopdir).
        assert!(!ff_path_in_stoplist(b"/home/r", &[b"/home/rks"]));
    }

    #[test]
    fn ff_path_in_stoplist_no_match() {
        assert!(!ff_path_in_stoplist(b"/etc", &[b"/home", b"/usr"]));
    }

    #[test]
    fn ff_path_in_stoplist_trailing_separators_are_trimmed() {
        assert!(ff_path_in_stoplist(b"/home/", &[b"/home"]));
    }
}
