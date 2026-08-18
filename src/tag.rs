//! Translated from `src/nvim/tag.c` (tractable core only).
//!
//! `tag.c` (~3900 lines) is the `:tag`/tag-stack/tags-file navigation
//! subsystem - almost all of it needs the tags-file search engine
//! (`find_tags`), `:tag`/`:pop`/`:tselect` Ex-command handling, and
//! real buffer switching/window management, none of which are
//! translated yet. Translated: the read-only tag-stack introspection
//! needed by the `gettagstack()` builtin (`get_tag_details`/
//! [`get_tagstack`]), plus [`tag_strnicmp`] (a small, pure,
//! case-insensitive comparator used for tag sorting) and
//! [`matching_line_len`] (walks a matching tag line's two consecutive
//! NUL-terminated strings; clamped to the slice length so a buffer
//! missing a terminator cannot report a length past its own end, which
//! the original's raw pointer arithmetic would).
//!
//! `tag.c`'s own `tagstack_clear_entry` was already translated in an
//! earlier session, hosted in `mark.rs` alongside its only real
//! consumer (`mark_forget_file`) rather than waiting for the rest of
//! this file - see that module's own doc comment.
//!
//! Also translated: [`did_set_tagfunc`]/[`set_buflocal_tfu_callback`]/
//! [`set_ref_in_tagfunc`]/[`free_tagfunc_option`] - parse, copy, mark,
//! or release global and buffer-local `'tagfunc'` callbacks.
//!
//! Also translated: the tag-stack WRITE side - [`set_tagstack`] (the
//! `settagstack()` builtin's real dispatcher) plus its own private
//! helpers `tagstack_clear`/`tagstack_shift`/`tagstack_push_item`/
//! `tagstack_push_items`/`tagstack_set_curidx`. Unblocked now that
//! `eval/eval.rs`'s `list2fpos` exists (called with `charcol = false`
//! here, which never reaches that function's own still-deferred
//! `buflist_findnr`-needing branch). `tfu_in_use` (a plain file-static
//! `bool`, `TFU_IN_USE` here) matches `autocmd.rs`'s `AUTOCMD_BUSY`
//! precedent for a currently-always-`false` guard flag: its only real
//! setter, `find_tagfunc_tags`, needs the funcref-call machinery, not
//! translated. The original's own `emsg()` calls (invalid-argument-
//! type/recursive-tagfunc-modification errors) have their message
//! display skipped, keeping the exact same `FAIL`/`OK` return value
//! and every other state change - matching this crate's established
//! policy.
//!
//! Also translated: [`tag_freematch`] - clears the cached tag-match
//! name.
//! `findtags_matchargs_init` initializes the six-field match state
//! used by the deferred tags-file search engine.

use crate::buffer_defs::{TaggyT, WinT};

/// Max. size of a line in the tags file (`LSIZE`, `tag.h`).
///
/// Also used as the per-entry scratch-buffer bound by
/// `optionstr.rs`'s own `did_set_complete`, matching the original's
/// own cross-file use of this same constant.
pub const LSIZE: usize = 512;
/// Treat the tag pattern as a regular expression (`TAG_REGEXP`).
const TAG_REGEXP: i32 = 4;

/// Byte offsets into one tags-file line (`tagptrs_T`).
#[allow(dead_code)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct TagPtrsT {
    tagname: Option<usize>,
    tagname_end: Option<usize>,
    fname: Option<usize>,
    fname_end: Option<usize>,
    command: Option<usize>,
    command_end: Option<usize>,
    tag_fname: Option<usize>,
    tagkind: Option<usize>,
    tagkind_end: Option<usize>,
    user_data: Option<usize>,
    user_data_end: Option<usize>,
    tagline: crate::pos_defs::LinenrT,
}

/// Parse tag-name, file-name and command boundaries from one tags-file
/// line (`parse_tag_line`).
#[allow(dead_code)]
fn parse_tag_line(line: &[u8], tag: &mut TagPtrsT) -> i32 {
    tag.tagname = Some(0);
    let Some(first_tab) = line.iter().position(|&byte| byte == b'\t') else {
        return crate::vim_defs::FAIL;
    };
    tag.tagname_end = Some(first_tab);

    let fname = first_tab + 1;
    tag.fname = Some(fname);
    let Some(second_rel) = line[fname..].iter().position(|&byte| byte == b'\t') else {
        return crate::vim_defs::FAIL;
    };
    let second_tab = fname + second_rel;
    tag.fname_end = Some(second_tab);

    let command = second_tab + 1;
    if command >= line.len() || line[command] == 0 {
        return crate::vim_defs::FAIL;
    }
    tag.command = Some(command);
    crate::vim_defs::OK
}

/// Arguments used for matching one tags-file line
/// (`findtags_match_args_T`).
#[derive(Debug, Default, PartialEq, Eq)]
struct FindtagsMatchArgsT {
    matchoff: i32,
    match_re: bool,
    match_no_ic: bool,
    has_re: bool,
    sortic: bool,
    sort_error: bool,
}

/// Initialize tags-file matching state (`findtags_matchargs_init`).
#[allow(dead_code)]
fn findtags_matchargs_init(margs: &mut FindtagsMatchArgsT, flags: i32) {
    margs.matchoff = 0;
    margs.match_re = false;
    margs.match_no_ic = false;
    margs.has_re = flags & TAG_REGEXP != 0;
    margs.sortic = false;
    margs.sort_error = false;
}

/// Cached tag match name (`tagmatchname`).
static TAGMATCHNAME: crate::globals::GlobalCell<Option<Vec<u8>>> =
    crate::globals::GlobalCell::new(None);

/// Free the cached tag match (`tag_freematch`).
///
/// # Safety
/// Must not run concurrently with another access to `TAGMATCHNAME`.
pub unsafe fn tag_freematch() {
    // SAFETY: forwarded from this function's own safety doc.
    *unsafe { TAGMATCHNAME.get_mut() } = None;
}

/// Add the details of a single tag-stack entry to `retdict`
/// (`get_tag_details`).
fn get_tag_details(tag: &TaggyT, retdict: &mut crate::eval::typval_defs::DictT) {
    use crate::eval::typval::{tv_dict_add_list, tv_dict_add_nr, tv_dict_add_str, tv_list_alloc, tv_list_append_number};
    use crate::pos_defs::MAXCOL;

    tv_dict_add_str(retdict, b"tagname", Some(&tag.tagname));
    tv_dict_add_nr(retdict, b"matchnr", i64::from(tag.cur_match) + 1);
    tv_dict_add_nr(retdict, b"bufnr", i64::from(tag.cur_fnum));
    if let Some(user_data) = &tag.user_data {
        tv_dict_add_str(retdict, b"user_data", Some(user_data));
    }

    let pos = tv_list_alloc(4);
    // SAFETY: `pos` was just allocated above, not yet shared beyond
    // `retdict` (which only holds a refcounted reference via
    // `tv_dict_add_list`).
    unsafe {
        tv_dict_add_list(retdict, b"from", pos);

        let fmark = &tag.fmark;
        tv_list_append_number(pos, if fmark.fnum != -1 { i64::from(fmark.fnum) } else { 0 });
        tv_list_append_number(pos, i64::from(fmark.mark.lnum));
        tv_list_append_number(pos, i64::from(if fmark.mark.col == MAXCOL { MAXCOL } else { fmark.mark.col + 1 }));
        tv_list_append_number(pos, i64::from(fmark.mark.coladd));
    }
}

/// Return the tag stack entries of window `wp` in dictionary
/// `retdict` (`get_tagstack`).
///
/// # Safety
/// `retdict` must be a valid, non-null pointer to a freshly-allocated,
/// not-yet-shared `DictT` (matching [`crate::eval::typval::tv_dict_alloc_ret`]'s
/// own contract).
pub unsafe fn get_tagstack(wp: &WinT, retdict: *mut crate::eval::typval_defs::DictT) {
    use crate::eval::typval::{tv_dict_add_list, tv_dict_add_nr, tv_list_alloc, tv_list_append_dict};

    // SAFETY: forwarded from this function's own safety doc.
    let retdict_ref = unsafe { &mut *retdict };
    tv_dict_add_nr(retdict_ref, b"length", i64::from(wp.w_tagstacklen));
    tv_dict_add_nr(retdict_ref, b"curidx", i64::from(wp.w_tagstackidx) + 1);
    let l = tv_list_alloc(2);
    // SAFETY: `l` was just allocated above, not yet shared beyond
    // `retdict` (which only holds a refcounted reference).
    unsafe { tv_dict_add_list(retdict_ref, b"items", l) };

    for i in 0..wp.w_tagstacklen as usize {
        let d = crate::eval::typval::tv_dict_alloc();
        // SAFETY: `d`/`l` are both valid, freshly-obtained live
        // pointers (`l` just allocated above; `tv_dict_alloc` never
        // returns null).
        unsafe { tv_list_append_dict(l, d) };
        // SAFETY: `d` was just returned by `tv_dict_alloc` above, not
        // yet shared beyond `l`.
        get_tag_details(&wp.w_tagstack[i], unsafe { &mut *d });
    }
}

/// The length of a matching tag line (`matching_line_len`).
///
/// The buffer holds a leading flag byte followed by TWO consecutive
/// NUL-terminated strings, so this walks past the first string's
/// terminator and then adds the second string's own length. The
/// result therefore covers both strings and the separator between
/// them, but NOT the final terminator.
///
/// The original does this with pointer arithmetic over a real
/// NUL-terminated buffer; here the terminators are located by
/// searching, and the result is clamped to the slice length, so a
/// buffer missing a terminator can never report a length past its own
/// end.
#[must_use]
pub fn matching_line_len(lbuf: &[u8]) -> usize {
    // Skip the leading flag byte.
    let first = lbuf.get(1..).unwrap_or(&[]);
    let first_len = first
        .iter()
        .position(|&c| c == crate::ascii_defs::NUL)
        .unwrap_or(first.len());

    // Past the first string and its terminator.
    let second_start = 1 + first_len + 1;
    let second = lbuf.get(second_start..).unwrap_or(&[]);
    let second_len = second
        .iter()
        .position(|&c| c == crate::ascii_defs::NUL)
        .unwrap_or(second.len());

    (second_start + second_len).min(lbuf.len())
}

/// Compares `s1`/`s2` for `len` bytes, ignoring ASCII case (folding to
/// uppercase, matching `'sort -f'`) - `0` for a match, negative if
/// `s1` sorts first, positive if `s2` sorts first (`tag_strnicmp`).
///
/// A byte beyond either slice's own length (when the other is longer)
/// is treated as `0`/NUL, matching how a shorter, real NUL-terminated
/// C string would naturally compare here.
#[must_use]
pub fn tag_strnicmp(s1: &[u8], s2: &[u8], len: usize) -> i32 {
    for k in 0..len {
        let c1 = s1.get(k).copied().unwrap_or(0);
        let c2 = s2.get(k).copied().unwrap_or(0);
        let diff =
            crate::macros_defs::toupper_asc(i32::from(c1)) - crate::macros_defs::toupper_asc(i32::from(c2));
        if diff != 0 {
            return diff;
        }
        if c1 == 0 {
            break;
        }
    }
    0
}

/// The `'tagfunc'` callback (`tfu_cb`, a file-static `Callback`).
static TFU_CB: crate::globals::GlobalCell<crate::eval::typval_defs::Callback> =
    crate::globals::GlobalCell::new(crate::eval::typval_defs::Callback::None);

/// Process the `'tagfunc'` option (`did_set_tagfunc`).
///
/// # Safety
/// `args.os_buf` must point to a live buffer. Touches global/local
/// callback and function-reference state.
pub unsafe fn did_set_tagfunc(
    args: &mut crate::option_defs::OptsetT,
) -> Option<&'static [u8]> {
    let buf = args.os_buf.cast::<crate::buffer_defs::BufT>();
    assert!(!buf.is_null(), "did_set_tagfunc: missing buffer");
    let flags = args.os_flags as u32;
    let retval = if flags & crate::option_defs::opt_set_flags::OPT_LOCAL != 0 {
        let value = unsafe { (*buf).b_p_tfu.clone() };
        crate::option::option_set_callback_func(
            value.as_deref(),
            unsafe { &mut (*buf).b_tfu_cb },
        )
    } else {
        let value =
            unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_tfu.clone();
        let retval = crate::option::option_set_callback_func(
            value.as_deref(),
            unsafe { TFU_CB.get_mut() },
        );
        if retval == crate::vim_defs::OK
            && flags & crate::option_defs::opt_set_flags::OPT_GLOBAL == 0
        {
            unsafe { set_buflocal_tfu_callback(buf) };
        }
        retval
    };
    if retval == crate::vim_defs::FAIL {
        Some(crate::errors::e_invarg.as_bytes())
    } else {
        None
    }
}

/// Copy the global `'tagfunc'` callback to a buffer-local cache
/// (`set_buflocal_tfu_callback`).
///
/// # Safety
/// `buf` and all callback referents must remain valid.
pub unsafe fn set_buflocal_tfu_callback(
    buf: *mut crate::buffer_defs::BufT,
) {
    crate::eval::typval::callback_free(unsafe { &mut (*buf).b_tfu_cb });
    let global = unsafe { &*TFU_CB.as_ptr() };
    if global.kind() != crate::eval::typval_defs::CallbackType::None {
        unsafe {
            crate::eval::typval::callback_copy(
                &mut (*buf).b_tfu_cb,
                global,
            )
        };
    }
}

/// Release the global `'tagfunc'` callback (`free_tagfunc_option`).
///
/// # Safety
/// Must not run concurrently with another access to `TFU_CB`.
pub unsafe fn free_tagfunc_option() {
    // SAFETY: forwarded from this function's own safety doc.
    crate::eval::typval::callback_free(unsafe { TFU_CB.get_mut() });
}

/// Mark the global `'tagfunc'` callback with `copy_id` so that it is
/// not garbage collected (`set_ref_in_tagfunc`).
///
/// # Safety
/// Same as [`crate::eval::eval::set_ref_in_callback`].
pub unsafe fn set_ref_in_tagfunc(copy_id: i32) -> bool {
    // SAFETY: forwarded from this function's own safety doc.
    let cb = unsafe { &*TFU_CB.as_ptr() };
    // SAFETY: forwarded from this function's own safety doc.
    unsafe { crate::eval::eval::set_ref_in_callback(cb, copy_id, std::ptr::null_mut(), std::ptr::null_mut()) }
}

/// `tfu_in_use` - disallow modifying the tag stack from inside a
/// `'tagfunc'` callback (`TFU_IN_USE`, a plain file-static `bool`).
/// Always `false` today: its only real setter, `find_tagfunc_tags`,
/// needs the funcref-call machinery, not translated - matches
/// `autocmd.rs`'s `AUTOCMD_BUSY` precedent for a currently-always-
/// `false` guard flag.
static TFU_IN_USE: crate::globals::GlobalCell<bool> = crate::globals::GlobalCell::new(false);

/// Free the current tag stack of window `wp` (`tagstack_clear`).
fn tagstack_clear(wp: &mut WinT) {
    for i in 0..wp.w_tagstacklen as usize {
        crate::mark::tagstack_clear_entry(&mut wp.w_tagstack[i]);
    }
    wp.w_tagstacklen = 0;
    wp.w_tagstackidx = 0;
}

/// Remove the oldest entry from the tag stack and shift the rest of
/// the entries down to fill the gap (`tagstack_shift`).
fn tagstack_shift(wp: &mut WinT) {
    crate::mark::tagstack_clear_entry(&mut wp.w_tagstack[0]);
    for i in 1..wp.w_tagstacklen as usize {
        wp.w_tagstack[i - 1] = wp.w_tagstack[i].clone();
    }
    wp.w_tagstacklen -= 1;
}

/// Push a new item to the tag stack of window `wp`
/// (`tagstack_push_item`).
#[allow(clippy::too_many_arguments)]
fn tagstack_push_item(
    wp: &mut WinT,
    tagname: Vec<u8>,
    cur_fnum: i32,
    cur_match: i32,
    mark: crate::pos_defs::PosT,
    fnum: i32,
    user_data: Option<Vec<u8>>,
) {
    let mut idx = wp.w_tagstacklen as usize; // top of the stack

    // if the tagstack is full: remove the oldest entry
    if idx >= crate::mark_defs::TAGSTACKSIZE as usize {
        tagstack_shift(wp);
        idx = crate::mark_defs::TAGSTACKSIZE as usize - 1;
    }

    wp.w_tagstacklen += 1;
    wp.w_tagstack[idx] = TaggyT {
        tagname,
        cur_fnum,
        cur_match: cur_match.max(0),
        fmark: crate::mark_defs::FmarkT {
            mark,
            fnum,
            view: crate::mark_defs::FmarkvT::default(),
            ..Default::default()
        },
        user_data,
    };
}

/// Add a list of items (each a Vimscript dict with `from`/`tagname`/
/// `bufnr`/`matchnr`/`user_data` keys) to the tag stack of window `wp`
/// (`tagstack_push_items`). Any non-dict item, or a dict missing
/// `from`/`tagname`, is silently skipped, matching the original's own
/// `continue` on every such case.
///
/// # Safety
/// `l` must be a valid, non-null pointer to a live `ListT`.
unsafe fn tagstack_push_items(wp: &mut WinT, l: *mut crate::eval::typval_defs::ListT) {
    use crate::eval::typval::{tv_dict_find, tv_dict_get_number, tv_dict_get_string, tv_list_first};
    use crate::eval::typval_defs::TypvalValue;

    // SAFETY: forwarded from this function's own safety doc.
    let mut li = unsafe { tv_list_first(l) };
    while !li.is_null() {
        // SAFETY: `li` is a valid, live list-item pointer for the
        // duration of this one iteration.
        let item_value = unsafe { &(*li).li_tv.value };
        let itemdict = match item_value {
            TypvalValue::Dict(d) if !d.is_null() => *d,
            _ => {
                // SAFETY: forwarded from this function's own safety doc.
                li = unsafe { (*li).li_next };
                continue;
            }
        };

        // SAFETY: `itemdict` is a valid, live dict pointer (checked
        // non-null above).
        let Some(from_di) = tv_dict_find(Some(unsafe { &mut *itemdict }), b"from") else {
            // SAFETY: forwarded from this function's own safety doc.
            li = unsafe { (*li).li_next };
            continue;
        };
        let mut mark = crate::pos_defs::PosT::default();
        let mut fnum = 0i32;
        // SAFETY: `from_di` is a valid, live dictitem pointer, just
        // found in `itemdict` above.
        let from_ok = unsafe {
            crate::eval::eval::list2fpos(&(*from_di).di_tv, &mut mark, Some(&mut fnum), None, false)
        } == crate::vim_defs::OK;
        if !from_ok {
            // SAFETY: forwarded from this function's own safety doc.
            li = unsafe { (*li).li_next };
            continue;
        }

        // SAFETY: `itemdict` is still valid.
        let Some(tagname) = (unsafe { tv_dict_get_string(Some(&mut *itemdict), b"tagname") }) else {
            // SAFETY: forwarded from this function's own safety doc.
            li = unsafe { (*li).li_next };
            continue;
        };

        if mark.col > 0 {
            mark.col -= 1;
        }

        // SAFETY: `itemdict` is still valid.
        let cur_fnum = unsafe { tv_dict_get_number(Some(&mut *itemdict), b"bufnr") } as i32;
        // SAFETY: `itemdict` is still valid.
        let cur_match = unsafe { tv_dict_get_number(Some(&mut *itemdict), b"matchnr") } as i32 - 1;
        // SAFETY: `itemdict` is still valid.
        let user_data = unsafe { tv_dict_get_string(Some(&mut *itemdict), b"user_data") };

        tagstack_push_item(wp, tagname, cur_fnum, cur_match, mark, fnum, user_data);

        // SAFETY: forwarded from this function's own safety doc.
        li = unsafe { (*li).li_next };
    }
}

/// Set the current index in window `wp`'s tag stack, clamped to
/// `[0, w_tagstacklen]` (`tagstack_set_curidx`).
fn tagstack_set_curidx(wp: &mut WinT, curidx: i32) {
    wp.w_tagstackidx = curidx.clamp(0, wp.w_tagstacklen);
}

/// Set the tag stack entries of window `wp` from a Vimscript dict
/// description (`set_tagstack`, the `settagstack()` builtin's real
/// dispatcher). `action` is one of `` b'a' `` (append), `` b'r' ``
/// (replace), or `` b't' `` (truncate).
///
/// The original's own `emsg()` calls (recursive-tagfunc-modification,
/// wrong-argument-type errors) have their message display skipped
/// (`message.c`'s pipeline not tractable), keeping the exact same
/// `FAIL`/`OK` return value and every other state change - matching
/// this module's established policy.
///
/// # Safety
/// `d` must be a valid, non-null pointer to a live `DictT`.
pub unsafe fn set_tagstack(wp: &mut WinT, d: *mut crate::eval::typval_defs::DictT, action: u8) -> i32 {
    use crate::eval::typval::{tv_dict_find, tv_get_number};
    use crate::eval::typval_defs::TypvalValue;
    use crate::vim_defs::{FAIL, OK};

    // SAFETY: forwarded from this function's own safety doc.
    if unsafe { *TFU_IN_USE.get_mut() } {
        return FAIL;
    }

    let mut l: *mut crate::eval::typval_defs::ListT = std::ptr::null_mut();

    // SAFETY: forwarded from this function's own safety doc.
    if let Some(items_di) = tv_dict_find(Some(unsafe { &mut *d }), b"items") {
        // SAFETY: `items_di` is a valid, live dictitem pointer, just
        // found in `d` above.
        let items_value = unsafe { &(*items_di).di_tv.value };
        let TypvalValue::List(items_list) = items_value else {
            return FAIL;
        };
        l = *items_list;
    }

    // SAFETY: `d` is still valid.
    if let Some(curidx_di) = tv_dict_find(Some(unsafe { &mut *d }), b"curidx") {
        // SAFETY: `curidx_di` is a valid, live dictitem pointer, just
        // found in `d` above.
        let n = tv_get_number(unsafe { &(*curidx_di).di_tv });
        tagstack_set_curidx(wp, n as i32 - 1);
    }

    if action == b't' {
        // truncate the stack: delete every entry above the current one
        let tagstackidx = wp.w_tagstackidx;
        let mut tagstacklen = wp.w_tagstacklen;
        while tagstackidx < tagstacklen {
            tagstacklen -= 1;
            crate::mark::tagstack_clear_entry(&mut wp.w_tagstack[tagstacklen as usize]);
        }
        wp.w_tagstacklen = tagstacklen;
    }

    if !l.is_null() {
        if action == b'r' {
            // replace the stack
            tagstack_clear(wp);
        }
        // SAFETY: forwarded from this function's own safety doc.
        unsafe { tagstack_push_items(wp, l) };
        // set the current index after the last entry
        wp.w_tagstackidx = wp.w_tagstacklen;
    }

    OK
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tag_line_finds_name_file_and_command_boundaries() {
        let line = b"main\tsrc/main.c\t/^fn main()$/\0";
        let mut tag = TagPtrsT::default();
        assert_eq!(parse_tag_line(line, &mut tag), crate::vim_defs::OK);
        assert_eq!(&line[tag.tagname.unwrap()..tag.tagname_end.unwrap()], b"main");
        assert_eq!(&line[tag.fname.unwrap()..tag.fname_end.unwrap()], b"src/main.c");
        assert_eq!(&line[tag.command.unwrap()..line.len() - 1], b"/^fn main()$/");
    }

    #[test]
    fn parse_tag_line_rejects_missing_fields_or_command() {
        assert_eq!(
            parse_tag_line(b"main src/main.c", &mut TagPtrsT::default()),
            crate::vim_defs::FAIL
        );
        assert_eq!(
            parse_tag_line(b"main\tsrc/main.c\t\0", &mut TagPtrsT::default()),
            crate::vim_defs::FAIL
        );
    }

    struct TfuGuard {
        saved: Option<crate::eval::typval_defs::Callback>,
    }
    struct TfuOptionGuard(Option<Vec<u8>>);

    impl TfuGuard {
        fn install(value: crate::eval::typval_defs::Callback) -> Self {
            let slot = unsafe { &mut *TFU_CB.as_ptr() };
            Self { saved: Some(std::mem::replace(slot, value)) }
        }
    }

    impl Drop for TfuGuard {
        fn drop(&mut self) {
            let slot = unsafe { &mut *TFU_CB.as_ptr() };
            crate::eval::typval::callback_free(slot);
            *slot = self.saved.take().expect("saved callback");
        }
    }

    impl TfuOptionGuard {
        fn install(value: Option<&[u8]>) -> Self {
            Self(std::mem::replace(
                &mut unsafe { crate::option_vars::OPTION_VARS.get_mut() }
                    .p_tfu,
                value.map(<[u8]>::to_vec),
            ))
        }
    }

    impl Drop for TfuOptionGuard {
        fn drop(&mut self) {
            unsafe { crate::option_vars::OPTION_VARS.get_mut() }.p_tfu =
                self.0.take();
        }
    }

    struct TagmatchGuard(Option<Vec<u8>>);

    impl TagmatchGuard {
        fn install(value: Option<Vec<u8>>) -> Self {
            let saved = std::mem::replace(unsafe { TAGMATCHNAME.get_mut() }, value);
            Self(saved)
        }
    }

    impl Drop for TagmatchGuard {
        fn drop(&mut self) {
            *unsafe { TAGMATCHNAME.get_mut() } = self.0.take();
        }
    }

    #[test]
    fn tag_freematch_clears_the_cached_match_name() {
        let _lock = crate::globals::global_state_test_lock();
        let _guard = TagmatchGuard::install(Some(b"cached-tag".to_vec()));
        unsafe { tag_freematch() };
        assert!(unsafe { TAGMATCHNAME.get_mut() }.is_none());
    }

    #[test]
    fn findtags_matchargs_init_resets_state_and_tracks_regexp_flag() {
        let mut args = FindtagsMatchArgsT {
            matchoff: 9,
            match_re: true,
            match_no_ic: true,
            has_re: false,
            sortic: true,
            sort_error: true,
        };
        findtags_matchargs_init(&mut args, 0);
        assert_eq!(args, FindtagsMatchArgsT::default());

        findtags_matchargs_init(&mut args, TAG_REGEXP);
        assert_eq!(
            args,
            FindtagsMatchArgsT {
                has_re: true,
                ..Default::default()
            }
        );

        findtags_matchargs_init(&mut args, TAG_REGEXP | 0x4000);
        assert!(args.has_re);
    }
    use crate::eval::typval_defs::{DictitemT, TypvalT, TypvalValue};

    #[test]
    fn matching_line_len_spans_both_strings_and_the_separator() {
        // A flag byte, then two NUL-terminated strings. The result
        // covers both plus the separator, but not the final NUL.
        let lbuf = b"\x01abc\0def\0";
        assert_eq!(matching_line_len(lbuf), 8);
    }

    #[test]
    fn matching_line_len_handles_an_empty_first_string() {
        let lbuf = b"\x01\0def\0";
        assert_eq!(matching_line_len(lbuf), 5);
    }

    #[test]
    fn matching_line_len_handles_an_empty_second_string() {
        let lbuf = b"\x01abc\0\0";
        assert_eq!(matching_line_len(lbuf), 5);
    }

    #[test]
    fn matching_line_len_clamps_when_a_terminator_is_missing() {
        // The original walks a real NUL-terminated buffer, so a slice
        // without one has no defined answer. Clamping to the slice
        // length keeps the result from exceeding the input, which the
        // raw pointer arithmetic would otherwise do.
        assert_eq!(matching_line_len(b"\x01abc"), 4);
        assert_eq!(matching_line_len(b"\x01"), 1);
        assert_eq!(matching_line_len(b""), 0);
    }

    fn dict_get<'a>(d: &'a crate::eval::typval_defs::DictT, key: &[u8]) -> Option<&'a TypvalT> {
        // SAFETY: `d.dv_index` only ever holds still-live `DictitemT`
        // pointers owned by `d` itself.
        unsafe { d.dv_index.values().map(|&p| &*p).find(|item: &&DictitemT| item.di_key.starts_with(key)) }
            .map(|item| &item.di_tv)
    }

    fn list_items(l: *mut crate::eval::typval_defs::ListT) -> Vec<TypvalT> {
        let mut out = Vec::new();
        // SAFETY: `l` is a valid, live list pointer for the whole
        // duration of this walk.
        unsafe {
            let mut item = (*l).lv_first;
            while !item.is_null() {
                out.push((*item).li_tv.clone());
                item = (*item).li_next;
            }
        }
        out
    }

    // --- get_tag_details ---

    #[test]
    fn get_tag_details_populates_all_fields() {
        let _lock = crate::globals::global_state_test_lock();
        let tag = TaggyT {
            tagname: b"my_func".to_vec(),
            fmark: crate::mark_defs::FmarkT {
                mark: crate::pos_defs::PosT { lnum: 12, col: 3, coladd: 0 },
                fnum: 5,
                ..Default::default()
            },
            cur_match: 1,
            cur_fnum: 5,
            user_data: Some(b"udata".to_vec()),
        };

        let d = crate::eval::typval::tv_dict_alloc();
        // SAFETY: `d` was just allocated, exclusively owned here.
        get_tag_details(&tag, unsafe { &mut *d });

        // SAFETY: `d` is still a valid, exclusively-held pointer.
        let d_ref = unsafe { &*d };
        assert_eq!(dict_get(d_ref, b"tagname").unwrap().value, TypvalValue::String(Some(b"my_func".to_vec())));
        assert_eq!(dict_get(d_ref, b"matchnr").unwrap().value, TypvalValue::Number(2));
        assert_eq!(dict_get(d_ref, b"bufnr").unwrap().value, TypvalValue::Number(5));
        assert_eq!(dict_get(d_ref, b"user_data").unwrap().value, TypvalValue::String(Some(b"udata".to_vec())));

        let TypvalValue::List(from) = dict_get(d_ref, b"from").unwrap().value else { panic!("expected a List") };
        let items = list_items(from);
        assert_eq!(items.len(), 4);
        assert_eq!(items[0].value, TypvalValue::Number(5));
        assert_eq!(items[1].value, TypvalValue::Number(12));
        assert_eq!(items[2].value, TypvalValue::Number(4)); // col + 1
        assert_eq!(items[3].value, TypvalValue::Number(0));

        // SAFETY: `d` is still exclusively owned; nothing else
        // references it.
        unsafe { crate::eval::typval::tv_dict_free(d) };
    }

    #[test]
    fn get_tag_details_no_user_data_key_when_none() {
        let _lock = crate::globals::global_state_test_lock();
        let tag = TaggyT { tagname: b"f".to_vec(), user_data: None, ..Default::default() };

        let d = crate::eval::typval::tv_dict_alloc();
        // SAFETY: `d` was just allocated, exclusively owned here.
        get_tag_details(&tag, unsafe { &mut *d });

        // SAFETY: `d` is still a valid, exclusively-held pointer.
        assert!(dict_get(unsafe { &*d }, b"user_data").is_none());

        // SAFETY: `d` is still exclusively owned; nothing else
        // references it.
        unsafe { crate::eval::typval::tv_dict_free(d) };
    }

    #[test]
    fn get_tag_details_maxcol_column_stays_maxcol() {
        let _lock = crate::globals::global_state_test_lock();
        let tag = TaggyT {
            tagname: b"f".to_vec(),
            fmark: crate::mark_defs::FmarkT {
                mark: crate::pos_defs::PosT { lnum: 1, col: crate::pos_defs::MAXCOL, coladd: 0 },
                fnum: -1,
                ..Default::default()
            },
            ..Default::default()
        };

        let d = crate::eval::typval::tv_dict_alloc();
        // SAFETY: `d` was just allocated, exclusively owned here.
        get_tag_details(&tag, unsafe { &mut *d });

        // SAFETY: `d` is still a valid, exclusively-held pointer.
        let d_ref = unsafe { &*d };
        let TypvalValue::List(from) = dict_get(d_ref, b"from").unwrap().value else { panic!("expected a List") };
        let items = list_items(from);
        assert_eq!(items[0].value, TypvalValue::Number(0), "fnum == -1 becomes 0");
        assert_eq!(items[2].value, TypvalValue::Number(i64::from(crate::pos_defs::MAXCOL)));

        // SAFETY: `d` is still exclusively owned; nothing else
        // references it.
        unsafe { crate::eval::typval::tv_dict_free(d) };
    }

    // --- get_tagstack ---

    #[test]
    fn get_tagstack_empty_stack() {
        let _lock = crate::globals::global_state_test_lock();
        let win = WinT::default();

        let mut rettv = TypvalT::default();
        // SAFETY: `rettv` is freshly default-initialized.
        let retdict = unsafe { crate::eval::typval::tv_dict_alloc_ret(&mut rettv) };
        // SAFETY: `retdict` was just freshly allocated by
        // `tv_dict_alloc_ret` above.
        unsafe { get_tagstack(&win, retdict) };

        // SAFETY: `retdict` is still a valid, exclusively-held pointer.
        let d_ref = unsafe { &*retdict };
        assert_eq!(dict_get(d_ref, b"length").unwrap().value, TypvalValue::Number(0));
        assert_eq!(dict_get(d_ref, b"curidx").unwrap().value, TypvalValue::Number(1));
        let TypvalValue::List(items) = dict_get(d_ref, b"items").unwrap().value else { panic!("expected a List") };
        assert_eq!(unsafe { crate::eval::typval::tv_list_len(items) }, 0);

        // SAFETY: `rettv` owns its own dict exclusively; nothing else
        // references it.
        unsafe { crate::eval::typval::tv_clear_simple(&rettv) };
    }

    #[test]
    fn get_tagstack_reports_entries_and_curidx() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT::default();
        win.w_tagstack[0] = TaggyT {
            tagname: b"foo".to_vec(),
            fmark: crate::mark_defs::FmarkT {
                mark: crate::pos_defs::PosT { lnum: 3, col: 0, coladd: 0 },
                fnum: 1,
                ..Default::default()
            },
            cur_match: 0,
            cur_fnum: 1,
            user_data: None,
        };
        win.w_tagstack[1] = TaggyT {
            tagname: b"bar".to_vec(),
            fmark: crate::mark_defs::FmarkT {
                mark: crate::pos_defs::PosT { lnum: 7, col: 0, coladd: 0 },
                fnum: 1,
                ..Default::default()
            },
            cur_match: 2,
            cur_fnum: 1,
            user_data: None,
        };
        win.w_tagstacklen = 2;
        win.w_tagstackidx = 1;

        let mut rettv = TypvalT::default();
        // SAFETY: `rettv` is freshly default-initialized.
        let retdict = unsafe { crate::eval::typval::tv_dict_alloc_ret(&mut rettv) };
        // SAFETY: `retdict` was just freshly allocated above.
        unsafe { get_tagstack(&win, retdict) };

        // SAFETY: `retdict` is still a valid, exclusively-held pointer.
        let d_ref = unsafe { &*retdict };
        assert_eq!(dict_get(d_ref, b"length").unwrap().value, TypvalValue::Number(2));
        assert_eq!(dict_get(d_ref, b"curidx").unwrap().value, TypvalValue::Number(2));
        let TypvalValue::List(items) = dict_get(d_ref, b"items").unwrap().value else { panic!("expected a List") };
        assert_eq!(unsafe { crate::eval::typval::tv_list_len(items) }, 2);

        let entries = list_items(items);
        let TypvalValue::Dict(d0) = entries[0].value else { panic!("expected a Dict") };
        // SAFETY: `d0` is a still-live dict owned by `items`.
        assert_eq!(dict_get(unsafe { &*d0 }, b"tagname").unwrap().value, TypvalValue::String(Some(b"foo".to_vec())));

        // SAFETY: `rettv` owns its own dict exclusively; nothing else
        // references it.
        unsafe { crate::eval::typval::tv_clear_simple(&rettv) };
    }

    #[test]
    fn tag_strnicmp_equal_strings() {
        assert_eq!(tag_strnicmp(b"Foo", b"foo", 3), 0);
    }

    #[test]
    fn tag_strnicmp_respects_len_bound() {
        // Only the first 3 bytes ("foo") are compared.
        assert_eq!(tag_strnicmp(b"foobar", b"foobaz", 3), 0);
    }

    #[test]
    fn tag_strnicmp_case_insensitive_difference() {
        assert!(tag_strnicmp(b"abc", b"abd", 3) < 0);
        assert!(tag_strnicmp(b"abd", b"abc", 3) > 0);
    }

    #[test]
    fn tag_strnicmp_shorter_string_compares_as_smaller() {
        // "ab" (hits its own end early) vs "abc": comparing 3 bytes,
        // the 3rd position is 0 (implicit NUL) vs 'c'.
        assert!(tag_strnicmp(b"ab", b"abc", 3) < 0);
        assert!(tag_strnicmp(b"abc", b"ab", 3) > 0);
    }

    #[test]
    fn tag_strnicmp_stops_at_embedded_nul_equivalent() {
        // Both empty: comparing for a len longer than either produces
        // 0 immediately (both bytes default to 0 at every position).
        assert_eq!(tag_strnicmp(b"", b"", 5), 0);
    }

    #[test]
    fn did_set_tagfunc_plain_set_updates_global_and_buffer_callbacks() {
        let _lock = crate::globals::global_state_test_lock();
        let _callback =
            TfuGuard::install(crate::eval::typval_defs::Callback::None);
        let _option = TfuOptionGuard::install(Some(b"GlobalTag"));
        let mut buf = Box::new(crate::buffer_defs::BufT::default());
        let buf_ptr = std::ptr::addr_of_mut!(*buf);
        let mut args = crate::option_defs::OptsetT {
            os_buf: buf_ptr.cast(),
            ..Default::default()
        };

        assert_eq!(unsafe { did_set_tagfunc(&mut args) }, None);
        assert!(matches!(
            unsafe { TFU_CB.get_mut() },
            crate::eval::typval_defs::Callback::Funcref(name)
                if name == b"GlobalTag"
        ));
        assert!(matches!(
            unsafe { &(*buf_ptr).b_tfu_cb },
            crate::eval::typval_defs::Callback::Funcref(name)
                if name == b"GlobalTag"
        ));
        assert!(!unsafe { set_ref_in_tagfunc(1) });
        crate::eval::typval::callback_free(unsafe {
            &mut (*buf_ptr).b_tfu_cb
        });
    }

    #[test]
    fn did_set_tagfunc_local_set_updates_only_buffer_callback() {
        let _lock = crate::globals::global_state_test_lock();
        let _callback =
            TfuGuard::install(crate::eval::typval_defs::Callback::None);
        let _option = TfuOptionGuard::install(Some(b"GlobalTag"));
        let mut buf = Box::new(crate::buffer_defs::BufT {
            b_p_tfu: Some(b"LocalTag".to_vec()),
            ..Default::default()
        });
        let buf_ptr = std::ptr::addr_of_mut!(*buf);
        let mut args = crate::option_defs::OptsetT {
            os_buf: buf_ptr.cast(),
            os_flags: crate::option_defs::opt_set_flags::OPT_LOCAL as i32,
            ..Default::default()
        };

        assert_eq!(unsafe { did_set_tagfunc(&mut args) }, None);
        assert_eq!(
            unsafe { TFU_CB.get_mut() }.kind(),
            crate::eval::typval_defs::CallbackType::None
        );
        assert!(matches!(
            unsafe { &(*buf_ptr).b_tfu_cb },
            crate::eval::typval_defs::Callback::Funcref(name)
                if name == b"LocalTag"
        ));
        crate::eval::typval::callback_free(unsafe {
            &mut (*buf_ptr).b_tfu_cb
        });
    }

    #[test]
    fn did_set_tagfunc_global_only_preserves_buffer_callback() {
        let _lock = crate::globals::global_state_test_lock();
        let _callback =
            TfuGuard::install(crate::eval::typval_defs::Callback::None);
        let _option = TfuOptionGuard::install(Some(b"NewGlobalTag"));
        let mut buf = Box::new(crate::buffer_defs::BufT::default());
        let buf_ptr = std::ptr::addr_of_mut!(*buf);
        crate::option::option_set_callback_func(
            Some(b"OldLocalTag"),
            unsafe { &mut (*buf_ptr).b_tfu_cb },
        );
        let mut args = crate::option_defs::OptsetT {
            os_buf: buf_ptr.cast(),
            os_flags: crate::option_defs::opt_set_flags::OPT_GLOBAL as i32,
            ..Default::default()
        };

        assert_eq!(unsafe { did_set_tagfunc(&mut args) }, None);
        assert!(matches!(
            unsafe { &(*buf_ptr).b_tfu_cb },
            crate::eval::typval_defs::Callback::Funcref(name)
                if name == b"OldLocalTag"
        ));
        crate::eval::typval::callback_free(unsafe {
            &mut (*buf_ptr).b_tfu_cb
        });
    }

    #[test]
    fn free_tagfunc_option_releases_and_clears_configured_callback() {
        let _lock = crate::globals::global_state_test_lock();
        let _callback =
            TfuGuard::install(crate::eval::typval_defs::Callback::None);
        let _option = TfuOptionGuard::install(Some(b"TagResolver"));
        let mut buf = crate::buffer_defs::BufT::default();
        let mut args = crate::option_defs::OptsetT {
            os_buf: std::ptr::from_mut(&mut buf).cast(),
            os_flags: crate::option_defs::opt_set_flags::OPT_GLOBAL as i32,
            ..Default::default()
        };
        unsafe { did_set_tagfunc(&mut args) };
        unsafe { free_tagfunc_option() };
        assert_eq!(
            unsafe { TFU_CB.get_mut() }.kind(),
            crate::eval::typval_defs::CallbackType::None
        );
    }

    // --- tagstack_clear / tagstack_shift / tagstack_push_item / tagstack_set_curidx ---

    fn make_taggy(tagname: &[u8], cur_fnum: i32, cur_match: i32, lnum: i32, fnum: i32) -> TaggyT {
        TaggyT {
            tagname: tagname.to_vec(),
            fmark: crate::mark_defs::FmarkT {
                mark: crate::pos_defs::PosT { lnum, col: 0, coladd: 0 },
                fnum,
                ..Default::default()
            },
            cur_match,
            cur_fnum,
            user_data: None,
        }
    }

    #[test]
    fn tagstack_clear_empties_the_stack_and_resets_indices() {
        let mut win = WinT::default();
        win.w_tagstack[0] = make_taggy(b"a", 1, 0, 1, 1);
        win.w_tagstack[1] = make_taggy(b"b", 2, 0, 2, 2);
        win.w_tagstacklen = 2;
        win.w_tagstackidx = 1;

        tagstack_clear(&mut win);

        assert_eq!(win.w_tagstacklen, 0);
        assert_eq!(win.w_tagstackidx, 0);
    }

    #[test]
    fn tagstack_shift_removes_the_oldest_entry_and_shifts_the_rest_down() {
        let mut win = WinT::default();
        win.w_tagstack[0] = make_taggy(b"a", 1, 0, 1, 1);
        win.w_tagstack[1] = make_taggy(b"b", 2, 0, 2, 2);
        win.w_tagstack[2] = make_taggy(b"c", 3, 0, 3, 3);
        win.w_tagstacklen = 3;

        tagstack_shift(&mut win);

        assert_eq!(win.w_tagstacklen, 2);
        assert_eq!(win.w_tagstack[0].tagname, b"b");
        assert_eq!(win.w_tagstack[1].tagname, b"c");
    }

    #[test]
    fn tagstack_push_item_appends_when_the_stack_is_not_full() {
        let mut win = WinT::default();
        let pos = crate::pos_defs::PosT { lnum: 4, col: 2, coladd: 0 };

        tagstack_push_item(&mut win, b"myfunc".to_vec(), 8, 1, pos, 7, None);

        assert_eq!(win.w_tagstacklen, 1);
        let entry = &win.w_tagstack[0];
        assert_eq!(entry.tagname, b"myfunc");
        assert_eq!(entry.cur_fnum, 8);
        assert_eq!(entry.cur_match, 1);
        assert_eq!(entry.fmark.mark, pos);
        assert_eq!(entry.fmark.fnum, 7);
        assert_eq!(entry.user_data, None);
    }

    #[test]
    fn tagstack_push_item_clamps_a_negative_cur_match_to_zero() {
        let mut win = WinT::default();
        tagstack_push_item(&mut win, b"f".to_vec(), 1, -1, crate::pos_defs::PosT::default(), 1, None);
        assert_eq!(win.w_tagstack[0].cur_match, 0);
    }

    #[test]
    fn tagstack_push_item_shifts_out_the_oldest_entry_once_the_stack_is_full() {
        let mut win = WinT::default();
        for i in 0..crate::mark_defs::TAGSTACKSIZE {
            let name = format!("tag{i}");
            tagstack_push_item(&mut win, name.into_bytes(), i, 0, crate::pos_defs::PosT::default(), i, None);
        }
        assert_eq!(win.w_tagstacklen, crate::mark_defs::TAGSTACKSIZE);
        assert_eq!(win.w_tagstack[0].tagname, b"tag0");

        // Pushing one more must shift "tag0" out; "tag1" becomes the
        // new oldest entry, and the new item lands at the top.
        tagstack_push_item(&mut win, b"tagNEW".to_vec(), 99, 0, crate::pos_defs::PosT::default(), 99, None);
        assert_eq!(win.w_tagstacklen, crate::mark_defs::TAGSTACKSIZE);
        assert_eq!(win.w_tagstack[0].tagname, b"tag1");
        assert_eq!(win.w_tagstack[(crate::mark_defs::TAGSTACKSIZE - 1) as usize].tagname, b"tagNEW");
    }

    #[test]
    fn tagstack_set_curidx_clamps_above_the_stack_length() {
        let mut win = WinT { w_tagstacklen: 5, ..Default::default() };
        tagstack_set_curidx(&mut win, 10);
        assert_eq!(win.w_tagstackidx, 5);
    }

    #[test]
    fn tagstack_set_curidx_clamps_below_zero() {
        let mut win = WinT { w_tagstacklen: 5, ..Default::default() };
        tagstack_set_curidx(&mut win, -3);
        assert_eq!(win.w_tagstackidx, 0);
    }

    #[test]
    fn tagstack_set_curidx_accepts_an_in_range_value() {
        let mut win = WinT { w_tagstacklen: 5, ..Default::default() };
        tagstack_set_curidx(&mut win, 3);
        assert_eq!(win.w_tagstackidx, 3);
    }

    // --- tagstack_push_items ---

    /// Builds a single tag-stack-item `Dict` matching `set_tagstack`'s
    /// own expected shape (the inverse of `get_tag_details`'s own
    /// construction): `{"from": [fnum, lnum, col, coladd], "tagname":
    /// ..., "bufnr": ..., "matchnr": ..., "user_data": ...}`.
    #[allow(clippy::too_many_arguments)]
    fn make_stack_item_dict(
        fnum: i64,
        lnum: i64,
        col: i64,
        coladd: i64,
        tagname: &[u8],
        bufnr: i64,
        matchnr: i64,
        user_data: Option<&[u8]>,
    ) -> *mut crate::eval::typval_defs::DictT {
        use crate::eval::typval::{tv_dict_add_list, tv_dict_add_nr, tv_dict_add_str, tv_list_alloc, tv_list_append_number};
        let d = crate::eval::typval::tv_dict_alloc();
        let from = tv_list_alloc(4);
        // SAFETY: `d`/`from` are both freshly allocated, not yet
        // shared beyond each other.
        unsafe {
            tv_dict_add_list(&mut *d, b"from", from);
            tv_list_append_number(from, fnum);
            tv_list_append_number(from, lnum);
            tv_list_append_number(from, col);
            tv_list_append_number(from, coladd);
        }
        // SAFETY: `d` is still exclusively owned here.
        let d_ref = unsafe { &mut *d };
        tv_dict_add_str(d_ref, b"tagname", Some(tagname));
        tv_dict_add_nr(d_ref, b"bufnr", bufnr);
        tv_dict_add_nr(d_ref, b"matchnr", matchnr);
        if let Some(ud) = user_data {
            tv_dict_add_str(d_ref, b"user_data", Some(ud));
        }
        d
    }

    #[test]
    fn tagstack_push_items_pushes_a_valid_entry_and_decrements_the_column() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT::default();
        let l = crate::eval::typval::tv_list_alloc(1);
        let d = make_stack_item_dict(7, 3, 5, 0, b"myfunc", 8, 2, None);
        // SAFETY: `l`/`d` are both freshly allocated, `d` not yet
        // shared beyond `l`.
        unsafe { crate::eval::typval::tv_list_append_dict(l, d) };

        // SAFETY: `l` is a valid, live, exclusively-referenced list.
        unsafe { tagstack_push_items(&mut win, l) };

        assert_eq!(win.w_tagstacklen, 1);
        let entry = &win.w_tagstack[0];
        assert_eq!(entry.tagname, b"myfunc");
        assert_eq!(entry.cur_fnum, 8);
        assert_eq!(entry.cur_match, 1); // matchnr(2) - 1
        assert_eq!(entry.fmark.fnum, 7);
        assert_eq!(entry.fmark.mark, crate::pos_defs::PosT { lnum: 3, col: 4, coladd: 0 }); // col(5) - 1

        // SAFETY: `l` is still exclusively owned; nothing else
        // references it.
        unsafe { crate::eval::typval::tv_list_unref(l) };
    }

    #[test]
    fn tagstack_push_items_col_zero_stays_zero() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT::default();
        let l = crate::eval::typval::tv_list_alloc(1);
        let d = make_stack_item_dict(1, 1, 0, 0, b"f", 1, 1, None);
        // SAFETY: `l`/`d` are both freshly allocated.
        unsafe { crate::eval::typval::tv_list_append_dict(l, d) };

        // SAFETY: `l` is a valid, live, exclusively-referenced list.
        unsafe { tagstack_push_items(&mut win, l) };

        assert_eq!(win.w_tagstack[0].fmark.mark.col, 0);

        // SAFETY: `l` is still exclusively owned.
        unsafe { crate::eval::typval::tv_list_unref(l) };
    }

    #[test]
    fn tagstack_push_items_carries_through_user_data() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT::default();
        let l = crate::eval::typval::tv_list_alloc(1);
        let d = make_stack_item_dict(1, 1, 1, 0, b"f", 1, 1, Some(b"extra"));
        // SAFETY: `l`/`d` are both freshly allocated.
        unsafe { crate::eval::typval::tv_list_append_dict(l, d) };

        // SAFETY: `l` is a valid, live, exclusively-referenced list.
        unsafe { tagstack_push_items(&mut win, l) };

        assert_eq!(win.w_tagstack[0].user_data, Some(b"extra".to_vec()));

        // SAFETY: `l` is still exclusively owned.
        unsafe { crate::eval::typval::tv_list_unref(l) };
    }

    #[test]
    fn tagstack_push_items_skips_a_non_dict_item() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT::default();
        let l = crate::eval::typval::tv_list_alloc(1);
        // SAFETY: `l` is freshly allocated.
        unsafe { crate::eval::typval::tv_list_append_number(l, 42) };

        // SAFETY: `l` is a valid, live, exclusively-referenced list.
        unsafe { tagstack_push_items(&mut win, l) };

        assert_eq!(win.w_tagstacklen, 0);

        // SAFETY: `l` is still exclusively owned.
        unsafe { crate::eval::typval::tv_list_unref(l) };
    }

    #[test]
    fn tagstack_push_items_skips_a_dict_missing_from() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT::default();
        let l = crate::eval::typval::tv_list_alloc(1);
        let d = crate::eval::typval::tv_dict_alloc();
        // SAFETY: `d` is freshly allocated.
        crate::eval::typval::tv_dict_add_str(unsafe { &mut *d }, b"tagname", Some(b"f"));
        // SAFETY: `l`/`d` are both freshly allocated.
        unsafe { crate::eval::typval::tv_list_append_dict(l, d) };

        // SAFETY: `l` is a valid, live, exclusively-referenced list.
        unsafe { tagstack_push_items(&mut win, l) };

        assert_eq!(win.w_tagstacklen, 0, "no 'from' key: item must be skipped");

        // SAFETY: `l` is still exclusively owned.
        unsafe { crate::eval::typval::tv_list_unref(l) };
    }

    #[test]
    fn tagstack_push_items_skips_a_dict_missing_tagname() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT::default();
        let l = crate::eval::typval::tv_list_alloc(1);
        let d = crate::eval::typval::tv_dict_alloc();
        let from = crate::eval::typval::tv_list_alloc(3);
        // SAFETY: `d`/`from` are both freshly allocated.
        unsafe {
            crate::eval::typval::tv_dict_add_list(&mut *d, b"from", from);
            crate::eval::typval::tv_list_append_number(from, 1);
            crate::eval::typval::tv_list_append_number(from, 1);
            crate::eval::typval::tv_list_append_number(from, 1);
        }
        // SAFETY: `l`/`d` are both freshly allocated.
        unsafe { crate::eval::typval::tv_list_append_dict(l, d) };

        // SAFETY: `l` is a valid, live, exclusively-referenced list.
        unsafe { tagstack_push_items(&mut win, l) };

        assert_eq!(win.w_tagstacklen, 0, "no 'tagname' key: item must be skipped");

        // SAFETY: `l` is still exclusively owned.
        unsafe { crate::eval::typval::tv_list_unref(l) };
    }

    #[test]
    fn tagstack_push_items_pushes_multiple_entries_in_order() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT::default();
        let l = crate::eval::typval::tv_list_alloc(2);
        let d1 = make_stack_item_dict(1, 1, 1, 0, b"first", 1, 1, None);
        let d2 = make_stack_item_dict(2, 2, 1, 0, b"second", 2, 1, None);
        // SAFETY: `l`/`d1`/`d2` are all freshly allocated.
        unsafe {
            crate::eval::typval::tv_list_append_dict(l, d1);
            crate::eval::typval::tv_list_append_dict(l, d2);
        }

        // SAFETY: `l` is a valid, live, exclusively-referenced list.
        unsafe { tagstack_push_items(&mut win, l) };

        assert_eq!(win.w_tagstacklen, 2);
        assert_eq!(win.w_tagstack[0].tagname, b"first");
        assert_eq!(win.w_tagstack[1].tagname, b"second");

        // SAFETY: `l` is still exclusively owned.
        unsafe { crate::eval::typval::tv_list_unref(l) };
    }

    // --- set_tagstack ---

    #[test]
    fn set_tagstack_fails_when_tfu_in_use() {
        let _lock = crate::globals::global_state_test_lock();
        // SAFETY: forwarded from TFU_IN_USE's own always-false doc -
        // poked directly to prove set_tagstack's own short-circuit is
        // faithfully translated, independent of how tfu_in_use
        // eventually gets set for real.
        unsafe { *TFU_IN_USE.get_mut() = true };
        let mut win = WinT::default();
        let d = crate::eval::typval::tv_dict_alloc();
        // SAFETY: `d` was just allocated, exclusively owned here; `win`
        // is a valid, live window.
        let rc = unsafe { set_tagstack(&mut win, d, b'r') };
        unsafe { *TFU_IN_USE.get_mut() = false };
        assert_eq!(rc, crate::vim_defs::FAIL);
        // SAFETY: `d` is still exclusively owned; nothing else
        // references it.
        unsafe { crate::eval::typval::tv_dict_free(d) };
    }

    #[test]
    fn set_tagstack_action_replace_clears_the_old_stack_before_pushing() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT::default();
        win.w_tagstack[0] = make_taggy(b"old", 1, 0, 1, 1);
        win.w_tagstacklen = 1;

        let d = crate::eval::typval::tv_dict_alloc();
        let items = crate::eval::typval::tv_list_alloc(1);
        let item = make_stack_item_dict(2, 2, 1, 0, b"new", 2, 1, None);
        // SAFETY: `d`/`items`/`item` are all freshly allocated, none
        // yet shared beyond each other.
        unsafe {
            crate::eval::typval::tv_dict_add_list(&mut *d, b"items", items);
            crate::eval::typval::tv_list_append_dict(items, item);
        }

        // SAFETY: `win` is valid; `d` was just freshly allocated.
        let rc = unsafe { set_tagstack(&mut win, d, b'r') };

        assert_eq!(rc, crate::vim_defs::OK);
        assert_eq!(win.w_tagstacklen, 1);
        assert_eq!(win.w_tagstack[0].tagname, b"new", "the old entry must be cleared, not kept alongside");
        assert_eq!(win.w_tagstackidx, 1);

        // SAFETY: `d` is still exclusively owned; nothing else
        // references it.
        unsafe { crate::eval::typval::tv_dict_free(d) };
    }

    #[test]
    fn set_tagstack_action_append_keeps_the_old_stack_and_appends() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT::default();
        win.w_tagstack[0] = make_taggy(b"old", 1, 0, 1, 1);
        win.w_tagstacklen = 1;

        let d = crate::eval::typval::tv_dict_alloc();
        let items = crate::eval::typval::tv_list_alloc(1);
        let item = make_stack_item_dict(2, 2, 1, 0, b"new", 2, 1, None);
        // SAFETY: `d`/`items`/`item` are all freshly allocated.
        unsafe {
            crate::eval::typval::tv_dict_add_list(&mut *d, b"items", items);
            crate::eval::typval::tv_list_append_dict(items, item);
        }

        // SAFETY: `win` is valid; `d` was just freshly allocated.
        let rc = unsafe { set_tagstack(&mut win, d, b'a') };

        assert_eq!(rc, crate::vim_defs::OK);
        assert_eq!(win.w_tagstacklen, 2);
        assert_eq!(win.w_tagstack[0].tagname, b"old");
        assert_eq!(win.w_tagstack[1].tagname, b"new");

        // SAFETY: `d` is still exclusively owned; nothing else
        // references it.
        unsafe { crate::eval::typval::tv_dict_free(d) };
    }

    #[test]
    fn set_tagstack_action_truncate_removes_entries_from_curidx_onward() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT::default();
        win.w_tagstack[0] = make_taggy(b"a", 1, 0, 1, 1);
        win.w_tagstack[1] = make_taggy(b"b", 2, 0, 2, 2);
        win.w_tagstack[2] = make_taggy(b"c", 3, 0, 3, 3);
        win.w_tagstacklen = 3;
        win.w_tagstackidx = 1;

        let d = crate::eval::typval::tv_dict_alloc();
        // SAFETY: `d` was just allocated, exclusively owned here.
        let rc = unsafe { set_tagstack(&mut win, d, b't') };

        assert_eq!(rc, crate::vim_defs::OK);
        // Entries at/above tagstackidx (1) are removed - only index 0
        // ("a") remains.
        assert_eq!(win.w_tagstacklen, 1);
        assert_eq!(win.w_tagstack[0].tagname, b"a");

        // SAFETY: `d` is still exclusively owned; nothing else
        // references it.
        unsafe { crate::eval::typval::tv_dict_free(d) };
    }

    #[test]
    fn set_tagstack_updates_curidx_from_the_dict_1_based() {
        let _lock = crate::globals::global_state_test_lock();
        let mut win = WinT::default();
        win.w_tagstack[0] = make_taggy(b"a", 1, 0, 1, 1);
        win.w_tagstack[1] = make_taggy(b"b", 2, 0, 2, 2);
        win.w_tagstack[2] = make_taggy(b"c", 3, 0, 3, 3);
        win.w_tagstacklen = 3;

        let d = crate::eval::typval::tv_dict_alloc();
        // SAFETY: `d` is freshly allocated.
        crate::eval::typval::tv_dict_add_nr(unsafe { &mut *d }, b"curidx", 2);
        // SAFETY: `d` was just allocated, exclusively owned here.
        let rc = unsafe { set_tagstack(&mut win, d, b'r') };

        assert_eq!(rc, crate::vim_defs::OK);
        // Vimscript's own 1-based "curidx" of 2 becomes the internal
        // 0-based index 1.
        assert_eq!(win.w_tagstackidx, 1);

        // SAFETY: `d` is still exclusively owned; nothing else
        // references it.
        unsafe { crate::eval::typval::tv_dict_free(d) };
    }
}
